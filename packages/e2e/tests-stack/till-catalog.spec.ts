import { expect, pairTill, test } from "../src/stack"
import { ownProduct, tile, toSale } from "./till"

test("an office price edit reaches an open sale screen on its next minute's sync", async ({
  page,
  request,
}) => {
  const { owner, register } = await pairTill(page, request)
  const product = await ownProduct(owner, register, "refresh")
  // The minute's refresh runs on the page's clock; no reload, as a till stays open all day.
  await page.clock.install()
  await toSale(page)
  const found = await tile(page, product.name)
  // 4.00 excl. 7 % sales tax.
  await expect(found).toContainText("4.28")

  await owner.post(owner.ws(`/products/${product.id}`), { price: "5.00" })
  await page.clock.fastForward(60_000)
  await expect(found).toContainText("5.35")
})
