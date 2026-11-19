import { activeBook, freshWorkspace } from "../../src/accounting"
import { psql } from "../../src/erp"
import { expect, loginOffice, stackAllow, test, urls } from "../../src/stack"
import { expectNoRawValues, pick } from "../../src/test"

/** Main store with 10 Honey jars on hand. */
async function stocked(api: Awaited<ReturnType<typeof freshWorkspace>>["api"]) {
  await api.post(api.ws("/warehouses"), {
    name: "Main store",
    default_warehouse: true,
  })
  await api.post(api.ws("/warehouses"), { name: "Back store" })
  const stores = await api.get<{ id: string; name: string }[]>(
    api.ws("/warehouses?page_size=100")
  )
  const main = stores.find((w) => w.name === "Main store")!
  const { product_id: product } = await api.post<{ product_id: string }>(
    api.ws("/products"),
    { name: "Honey jar", price: "12.50", has_stock: true }
  )
  await api.post(api.ws(`/products/${product}/stocks`), {
    warehouse_id: main.id,
    mutation_value: "10",
    reason: "Opening",
  })
  return { main, product }
}

test.describe("stock valuation", () => {
  let bare: Awaited<ReturnType<typeof freshWorkspace>>
  let day = ""
  test.beforeAll(async ({ request }) => {
    bare = await freshWorkspace(request, "valuation-bare")
    day = (
      await bare.api.get<{ business_date: string }>(
        bare.api.ws("/accounting/setup")
      )
    ).business_date
  })
  test.use({
    // eslint-disable-next-line no-empty-pattern -- Playwright reads fixture deps from the pattern
    allow: async ({}, use) =>
      use({
        ...stackAllow,
        // A book that is not set up yet is the case under test; the page says it in words.
        responses: [
          ...stackAllow.responses,
          {
            url: `${urls.office}/v1/workspace/${bare.api.workspace}/accounting/reports/inventory-valuation?as_of=${day}`,
            status: 422,
          },
        ],
      }),
  })

  test("stock value per product and warehouse against the inventory account", async ({
    page,
    request,
  }) => {
    const w = await freshWorkspace(request, "valuation")
    const { main, product } = await stocked(w.api)
    await activeBook(request, w.api)
    // shortcut: the opening value activation will seed (M8 route), as src/erp.ts draftCredit.
    psql(`INSERT INTO inventory_cost_pools (workspace_id, product_id, warehouse_id, quantity, value)
          VALUES ('${w.api.workspace}'::uuid, '${product}'::uuid, '${main.id}'::uuid, 10, 10000)`)
    await loginOffice(page, w.login)

    await page.goto(`${urls.office}/stock`)
    await page
      .getByRole("navigation", { name: "Stock" })
      .getByRole("link", { name: "Valuation" })
      .click()
    await expect(page.getByRole("heading", { name: "Valuation" })).toBeVisible()
    const row = page.getByRole("row", { name: /Honey jar/ })
    await expect(row).toContainText("Main store")
    await expect(row).toContainText("100.00")
    await expect(
      page.getByRole("row", { name: /Inventory account/ })
    ).toContainText("0.00")
    await expect(page.getByRole("row", { name: /Difference/ })).toContainText(
      "100.00"
    )
    await expect(page.getByText(/differ by/)).toBeVisible()
    await expectNoRawValues(page)

    // One warehouse: its rows only, and no GL comparison.
    await pick(page.getByLabel("Warehouse"), "Back store", "back")
    await page.getByRole("button", { name: "Show" }).click()
    await expect(page).toHaveURL(/warehouse=/)
    await expect(page.getByText("No stock value on this day")).toBeVisible()
    await expect(
      page.getByText(
        "The inventory account is compared for all warehouses together only."
      )
    ).toBeVisible()
    await page.getByRole("button", { name: "All warehouses" }).last().click()
    await expect(page.getByRole("row", { name: /Honey jar/ })).toBeVisible()

    // A day before the books started shows nothing.
    await page.getByLabel("On", { exact: true }).fill("2000-01-01")
    await page.getByRole("button", { name: "Show" }).click()
    await expect(page.getByText("No stock value on this day")).toBeVisible()
    await expectNoRawValues(page)
  })

  test("a book that is not set up yet is explained, not a raw error", async ({
    page,
  }) => {
    await loginOffice(page, bare.login)
    await page.goto(`${urls.office}/stock/valuation`)
    await expect(
      page.getByRole("alert").filter({ hasText: /\S/ })
    ).toBeVisible()
    await expectNoRawValues(page)
  })
})
