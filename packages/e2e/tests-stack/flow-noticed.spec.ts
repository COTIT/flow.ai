// F2 on the real stack: a product below its minimum shows in Flow noticed; acknowledging keeps
// it (the condition is still true); raising the stock above the minimum makes it disappear.
import scenesEn from "../../../apps/flow/messages/en/scenes.json" with { type: "json" }
import { expect, loginFlow, test, unique, urls } from "../src/stack"
import { rowAction } from "../tests/flow/lists"
import { freshOwner } from "./flow-scenes"

const sc = scenesEn.scenes

test("below minimum → acknowledge (still listed) → stock raised → gone", async ({
  page,
  request,
}) => {
  test.setTimeout(120_000)
  const { login, api } = await freshOwner(request, "noticed")
  // A new workspace starts without warehouses: open its store first.
  const { warehouse_id } = await api.post<{ warehouse_id: string }>(
    api.ws("/warehouses"),
    { name: "Shop floor", is_store: true }
  )
  const store = { id: warehouse_id }
  const name = `Raw Honey ${unique()}`
  const { product_id } = await api.post<{ product_id: string }>(
    api.ws("/products"),
    { name, price: "4.00" }
  )
  const product = { id: product_id }
  // 2 on hand, workspace threshold 5 (no per-row minimum): below minimum.
  const move = (change: number) =>
    api.post(api.ws(`/products/${product.id}/stocks`), {
      warehouse_id: store.id,
      mutation_value: change,
      reason: "count",
    })
  await move(2)

  await loginFlow(page, login)
  await page.goto(`${urls.flow}/scenes/noticed`)
  const item = page.locator(`[data-item="${product.id}"]`)
  await expect(item).toContainText(name)
  await rowAction(page, item, "acknowledge")
  await expect(item.locator("[data-acknowledged]:visible")).toBeVisible()
  await page.reload()
  await expect(item).toBeVisible()

  await move(10)
  await page.getByRole("button", { name: sc.refresh }).click()
  await expect(item).toHaveCount(0)
})
