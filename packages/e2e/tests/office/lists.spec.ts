import { expectNoHorizontalScroll, expectTapTargets } from "../../src/test"
import { expect, paged, test } from "./fake-api"

// Every list in the office uses the List template (docs/design/web-redesign, OfficeProducts):
// page head with the title, one glass card holding filters and the table, rows as cards at 375 px.
const lists: [path: string, title: string][] = [
  ["/products", "Products"],
  ["/stock", "Stock"],
  ["/stock/negative", "Stock"],
  ["/orders", "Orders"],
  ["/invoices", "Invoices"],
  ["/lots", "Batches"],
  ["/supply/links", "Supply"],
  ["/supply/transfers", "Supply"],
  ["/till/registers", "Till"],
  ["/till/users", "Till"],
  ["/till/vaults", "Till"],
  ["/till/shifts", "Till"],
  ["/till/sales", "Till"],
  ["/settings/app-keys", "App keys"],
  ["/settings/webhooks", "Webhooks"],
]

for (const [path, title] of lists) {
  test(`${path} uses the list template`, async ({ page, api }) => {
    // Every list (and every picker on it) answers empty.
    api.on("GET", "/workspace/:ws/.+", () => paged([]))
    await page.goto(path)
    await expect(page.getByRole("heading", { level: 1, name: title })).toBeVisible()
    const card = page.locator("[data-list]")
    await expect(card).toHaveCount(1)
    await expect(card.locator("[data-slot=empty-state]")).toBeVisible()
    await expectNoHorizontalScroll(page)
    if ((page.viewportSize()?.width ?? 0) < 768) await expectTapTargets(page)
  })
}

test("rows become cards on a phone", async ({ page, api }) => {
  api.on("GET", "/workspace/:ws/products", () =>
    paged([
      {
        id: "p1",
        name: "Wildflower honey 500 g",
        product_nr: "P-0001",
        reference: "AFM-WF-500",
        price_incl: "14.50",
        status: "active",
      },
    ])
  )
  await page.goto("/products")
  const row = page.getByRole("row", { name: /Wildflower honey/ })
  await expect(row).toBeVisible()
  const phone = (page.viewportSize()?.width ?? 0) < 768
  await expect(page.getByRole("columnheader", { name: "Name" })).toBeVisible({ visible: !phone })
  expect(await row.evaluate((el) => getComputedStyle(el).display)).toBe(phone ? "grid" : "table-row")
  await expect(page.getByRole("cell", { name: "14.50" })).toBeVisible()
})
