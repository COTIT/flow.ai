import { apiAs, cred, expect, test, unique, urls } from "../../src/stack"

type Warehouse = { id: string; is_store: boolean }

test.describe("stock gone between cart and order", () => {
  test("the shop says so in words by the line, and places nothing", async ({
    page,
    request,
  }) => {
    const owner = await apiAs(request, cred("office (AFM)", "owner"))
    // A product of the test's own, one in the web warehouse (data rule 1).
    const name = `e2e last jar ${unique()}`
    const { product_id: id } = await owner.post<{ product_id: string }>(
      owner.ws("/products"),
      {
        name,
        price: "6.00",
        store_only: false,
      }
    )
    const depot = (await owner.get<Warehouse[]>(owner.ws("/warehouses"))).find(
      (w) => !w.is_store
    )!
    const stock = (lvl: number) =>
      owner.post(owner.ws(`/products/${id}/set-stock-levels`), {
        warehouses: [{ warehouse_id: depot.id, stock_lvl: lvl, reason: "e2e" }],
      })
    await stock(1)

    await page.goto(`${urls.shop}/products/${id}`)
    await page.getByRole("button", { name: "Add to cart" }).click()
    await page.goto(`${urls.shop}/checkout`)
    await page.getByLabel("Email").fill(`buyer-${unique()}@example.test`)
    await page.getByLabel("Full name").fill("Ada Buyer")
    await page.getByLabel("Address", { exact: true }).fill("1 Main St")
    await page.getByLabel("Postal code").fill("10001")
    await page.getByLabel("City").fill("New York")
    await page.getByLabel("Country").fill("US")

    // Someone else buys the last one first.
    await stock(0)
    const before = await owner.get<unknown[]>(
      owner.ws(`/products/${id}/stock-movements?page_size=100`)
    )
    await page.getByRole("button", { name: "Place order" }).click()

    await expect(page.getByText(/not enough stock/i)).toBeVisible()
    await expect(page).toHaveURL(`${urls.shop}/checkout`)
    await expect(page.getByText(/\b422\b|lines\.\d|quantity:/)).toHaveCount(0)
    const after = await owner.get<unknown[]>(
      owner.ws(`/products/${id}/stock-movements?page_size=100`)
    )
    expect(after.length, "no stock held for a refused order").toBe(
      before.length
    )
  })
})
