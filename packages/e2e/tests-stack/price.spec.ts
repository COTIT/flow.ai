import type { Page } from "@playwright/test"

import { cred, expect, loginOffice, pairTill, test, unique, urls } from "../src/stack"

type Warehouse = { id: string; is_store: boolean }

const tile = (page: Page, name: string) =>
  page.getByRole("list", { name: "Products" }).getByRole("button", { name: new RegExp(name) })

async function tillTile(page: Page, name: string) {
  await page.goto(`${urls.till}/sale`)
  await page.getByLabel("Scan a barcode or type a name").fill(name)
  return tile(page, name)
}

test("an office price edit reaches the shop's product page and the till's tile", async ({ page, request }) => {
  test.setTimeout(150_000)
  // A product of the test's own (data rule 1), on sale in the till's store and the web warehouse.
  const { owner, register } = await pairTill(page, request)
  const name = `e2e price ${unique()}`
  const { product_id: id } = await owner.post<{ product_id: string }>(owner.ws("/products"), {
    name,
    price: "4.00",
    store_only: false,
  })
  const warehouses = await owner.get<Warehouse[]>(owner.ws("/warehouses"))
  const depot = warehouses.find((w) => !w.is_store)!
  await owner.post(owner.ws(`/products/${id}/set-stock-levels`), {
    warehouses: [
      { warehouse_id: register.warehouse_id, stock_lvl: 20, reason: "e2e" },
      { warehouse_id: depot.id, stock_lvl: 20, reason: "e2e" },
    ],
  })

  // Before: 4.00 excl. 7 % sales tax is 4.28 everywhere.
  await expect(await tillTile(page, name)).toContainText("4.28")
  await page.goto(`${urls.shop}/products/${id}`)
  await expect(page.getByRole("main")).toContainText("4.28")

  await loginOffice(page, cred("office (AFM)", "owner"))
  await page.goto(`${urls.office}/products/${id}`)
  await page.getByLabel("Price excl. VAT").fill("5.00")
  await page.getByRole("button", { name: "Save changes" }).click()
  await expect(page.getByText("Product saved.")).toBeVisible()

  // After: 5.35 in the shop (it keeps catalogue replies up to 60 s, so within about a minute) and on the till.
  await expect(async () => {
    await page.goto(`${urls.shop}/products/${id}`)
    await expect(page.getByRole("main")).toContainText("5.35", { timeout: 1_000 })
  }).toPass({ intervals: [5_000], timeout: 90_000 })
  await expect(await tillTile(page, name)).toContainText("5.35")
})
