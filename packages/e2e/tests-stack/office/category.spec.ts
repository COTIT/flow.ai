import { expectNoRawValues } from "../../src/test"
import { apiAs, cred, expect, loginOffice, test, unique, urls } from "../../src/stack"

type Warehouse = { id: string; is_store: boolean }

test("office: a new category, given to a product, filters the shop", async ({ page, request }) => {
  // Room for the shop's 60 s cache below.
  test.setTimeout(150_000)
  const owner = await apiAs(request, cred("office (AFM)", "owner"))
  const tag = unique()
  const category = `e2e cat ${tag}`
  const name = `e2e categorised ${tag}`
  const { product_id: id } = await owner.post<{ product_id: string }>(owner.ws("/products"), {
    name,
    price: "4.00",
  })
  // Web stock, so the shop lists it.
  const warehouses = await owner.get<Warehouse[]>(owner.ws("/warehouses"))
  const depot = warehouses.find((w) => !w.is_store)!
  await owner.post(owner.ws(`/products/${id}/set-stock-levels`), {
    warehouses: [{ warehouse_id: depot.id, stock_lvl: "5.00", reason: "e2e" }],
  })

  await loginOffice(page, cred("office (AFM)", "owner"))
  await page.goto(`${urls.office}/products/categories`)
  await page.getByRole("button", { name: "New category" }).click()
  const dialog = page.getByRole("dialog")
  await dialog.getByLabel("Name").fill(category)
  await dialog.getByLabel("Description").fill("Made by the e2e run")
  await dialog.getByRole("button", { name: "Save" }).click()
  await expect(page.getByRole("status").filter({ hasText: "Category created." })).toBeVisible()
  await expectNoRawValues(page)

  await page.goto(`${urls.office}/products/${id}`)
  await page.getByLabel("Category").selectOption({ label: category })
  await page.getByRole("button", { name: "Save changes" }).click()
  await expect(page.getByRole("status").filter({ hasText: "Product saved." })).toBeVisible()
  await expect(page.getByLabel("Category")).toHaveValue(/./)

  // The office list counts it under the category.
  await page.goto(`${urls.office}/products/categories`)
  await expect(page.getByRole("row").filter({ hasText: category })).toContainText("1")

  // The shop caches its category list for 60 s, so the new chip can take that long.
  await expect(async () => {
    await page.goto(`${urls.shop}/shop`)
    await page.getByRole("link", { name: category }).click({ timeout: 2_000 })
    await expect(page).toHaveURL(/category=/)
    await expect(page.getByText(name).first()).toBeVisible({ timeout: 2_000 })
  }).toPass({ timeout: 75_000 })
  await expectNoRawValues(page)
})
