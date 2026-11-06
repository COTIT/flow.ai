import { expectNoRawValues } from "../../src/test"
import {
  cred,
  expect,
  loginOffice,
  openShift,
  pairTill,
  test,
  unique,
  urls,
} from "../../src/stack"

type Warehouse = { id: string; name: string; is_store: boolean }

// The user's report: stock added at one place didn't change a -1 shown for another. Stock is
// kept per place; each screen must say which place a number is for, and why one is below zero.
test("office: stock added at one place and sold at another's till; both numbers explained", async ({
  page,
  request,
}) => {
  const { owner, register } = await pairTill(page, request)
  await openShift(page)
  // A product of the test's own with no stock anywhere yet.
  const name = `e2e places ${unique()}`
  const { product_id: id } = await owner.post<{ product_id: string }>(owner.ws("/products"), {
    name,
    price: "4.00",
    store_only: false,
  })
  const warehouses = await owner.get<Warehouse[]>(owner.ws("/warehouses?page_size=100"))
  const store = warehouses.find((w) => w.id === register.warehouse_id)!
  const depot = warehouses.find((w) => !w.is_store)!

  // The till sells one from its store, which has none recorded: never refused, goes to -1.
  await page.goto(`${urls.till}/sale`)
  await page.getByLabel("Scan a barcode or type a name").fill(name)
  await page.getByRole("list", { name: "Products" }).getByRole("button", { name: new RegExp(name) }).click()
  await page.getByRole("button", { name: /^Charge/ }).click()
  const sheet = page.getByRole("dialog", { name: "Take payment" })
  await sheet.getByRole("button", { name: "Cash" }).click()
  await sheet.getByRole("button", { name: "Exact" }).click()
  await sheet.getByRole("button", { name: /^Book sale/ }).click()
  await expect(page.getByRole("dialog", { name: "Done" })).toBeVisible()

  // The office adds 10 at the depot; the message names the place and its new number.
  await loginOffice(page, cred("office (AFM)", "owner"))
  await page.goto(`${urls.office}/products/${id}`)
  await page.getByRole("button", { name: "Change stock" }).click()
  const dialog = page.getByRole("dialog")
  await dialog.getByLabel("Warehouse").selectOption(depot.id)
  await expect(dialog.getByText(`Nothing at ${depot.name} yet.`)).toBeVisible()
  await dialog.getByLabel("Change (e.g. 10 or -3)").fill("10")
  await dialog.getByRole("button", { name: "Save" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: `Added 10 at ${depot.name}: now 10.` })
  ).toBeVisible()

  // The panel: each place with its own number; the store's -1 says why and how to fix it.
  const stock = page.getByRole("region", { name: "Stock", exact: true })
  await expect(stock).toContainText(depot.name)
  await expect(stock).toContainText(store.name)
  // The total is the API's sum of every place (10 + -1); each place keeps its own number.
  await expect(stock.getByText("9", { exact: true })).toBeVisible()
  await expect(stock.getByText("10", { exact: true })).toBeVisible()
  await expect(stock.locator("[data-slot=badge]").filter({ hasText: /^-1$/ })).toBeVisible()
  await expect(
    stock.getByText("More was sold here than was recorded. Count it and correct it.")
  ).toBeVisible()
  await expectNoRawValues(page)

  // Correct it opens Change stock at the store, showing its number.
  await stock.getByRole("button", { name: "Correct it" }).click()
  await expect(dialog.getByLabel("Warehouse")).toHaveValue(store.id)
  await expect(dialog.getByText(`Now at ${store.name}: -1`)).toBeVisible()
  await page.keyboard.press("Escape")

  // The Below zero list: the store row, explained, with the link back to the fix.
  await page.goto(`${urls.office}/stock/negative`)
  const row = page.getByRole("row").filter({ hasText: name })
  await expect(row).toContainText(store.name)
  await expect(row).toContainText("More was sold here than was recorded.")
  const fix = row.getByRole("link", { name: "Correct it" })
  await expect(fix).toHaveAttribute("href", `/products/${id}?change=${store.id}#product-stock`)
  await fix.click()
  await expect(page).toHaveURL(new RegExp(`/products/${id}\\?change=${store.id}`))
  await expect(page.getByRole("dialog").getByLabel("Warehouse")).toHaveValue(store.id)
  await expectNoRawValues(page)
})
