import { expectNoRawValues } from "../../src/test"
import {
  cred,
  expect,
  loginOffice,
  openShift,
  pairTill,
  stackAllow,
  test,
  unique,
  urls,
} from "../../src/stack"
import { tillApi } from "../till"

type Warehouse = { id: string; name: string; is_store: boolean }

type Movement = { quantity_change: string }

// The till's sale and quote are refused (422) on purpose here.
test.use({
  allow: {
    ...stackAllow,
    responses: [
      ...stackAllow.responses,
      { url: tillApi("/sales"), status: 422 },
      { url: tillApi("/sales/quote"), status: 422 },
    ],
  },
})

// Stock is kept per place, and a sale takes only what its own place has (masterplan D17): the
// till refuses what its store doesn't hold, even with stock at the depot; each office number
// says which place it is for.
test("office: the till refuses what its store lacks; stock added at the depot stays the depot's", async ({
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

  // The till tries to sell one from its store, which has none: refused, nothing booked.
  await page.goto(`${urls.till}/sale`)
  await page.getByLabel("Scan a barcode or type a name").fill(name)
  await page.getByRole("list", { name: "Products" }).getByRole("button", { name: new RegExp(name) }).click()
  await page.getByRole("button", { name: /^Charge/ }).click()
  const sheet = page.getByRole("dialog", { name: "Take payment" })
  await sheet.getByRole("button", { name: "Card" }).click()
  const refused = page.waitForResponse(
    (r) => r.url().includes("/till/sales") && r.request().method() === "POST" && !r.url().includes("quote")
  )
  await sheet.getByRole("button", { name: /^Book card sale/ }).click()
  const answer = await refused
  expect(answer.status()).toBe(422)
  expect((await answer.json()).code).toBe("insufficient_stock")
  await expect(page.getByRole("dialog", { name: "Done" })).toBeHidden()
  const moved = await owner.get<Movement[]>(owner.ws(`/products/${id}/stock-movements?page_size=100`))
  expect(moved, "no stock taken").toHaveLength(0)

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

  // The panel: each place with its own number; nothing is below zero.
  const stock = page.getByRole("region", { name: "Stock", exact: true })
  await expect(stock).toContainText(depot.name)
  await expect(stock.getByText("10", { exact: true }).first()).toBeVisible()
  await expect(stock.getByText("More was sold here than was recorded.")).toHaveCount(0)
  expect(store.id).not.toBe(depot.id)
  await expectNoRawValues(page)
})
