// F2 on the real stack: a product below its minimum shows in Flow noticed; acknowledging keeps
// it (the condition is still true); raising the stock above the minimum makes it disappear.
import scenesEn from "../../../apps/flow/messages/en/scenes.json" with { type: "json" }
import { expect, loginFlow, test, unique } from "../src/stack"
import { freshOwner } from "./flow-scenes"

const sc = scenesEn.scenes

test("below minimum → acknowledge (still listed) → stock raised → gone", async ({
  page,
  request,
}) => {
  test.setTimeout(120_000)
  const { login, api } = await freshOwner(request, "noticed")
  const store = (
    await api.get<{ id: string; is_store: boolean }[]>(api.ws("/warehouses"))
  ).find((w) => w.is_store)!
  const name = `Raw Honey ${unique()}`
  const product = await api.post<{ id: string }>(api.ws("/products"), {
    name,
    price: "4.00",
  })
  // 2 on hand, workspace threshold 5 (no per-row minimum): below minimum.
  const move = (change: number) =>
    api.post(api.ws(`/products/${product.id}/stocks`), {
      warehouse_id: store.id,
      mutation_value: change,
      reason: "count",
    })
  await move(2)

  await loginFlow(page, login)
  await page.goto("/scenes/noticed")
  const item = page.locator(`[data-item="${product.id}"]`)
  await expect(item).toContainText(name)
  await item.getByRole("button", { name: sc.noticed.acknowledge }).click()
  await expect(item.locator("[data-acknowledged]")).toBeVisible()
  await page.reload()
  await expect(item).toBeVisible()

  await move(10)
  await page.getByRole("button", { name: sc.refresh }).click()
  await expect(item).toHaveCount(0)
})
