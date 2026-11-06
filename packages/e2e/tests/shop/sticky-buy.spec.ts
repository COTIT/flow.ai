import { PRODUCT } from "../../mock/fixtures.mjs"
import {
  expect,
  expectNoHorizontalScroll,
  expectTapTargets,
  test,
} from "../../src/test"

const NAME = "Ilundo Wild Forest Honey"

test("on a phone, a bar keeps Add at hand once the button scrolls away", async ({
  page,
  isMobile,
}) => {
  test.skip(!isMobile, "phones only: desktop keeps the buy tile in view")
  await page.goto(`/products/${PRODUCT.honey}`)
  const bar = page.getByRole("region", { name: "Quick add" })
  await expect(page.getByRole("button", { name: "Add to cart" })).toBeEnabled()
  await expect(bar).toHaveCount(0)

  // The chosen size and quantity carry over; the bar shows the API's unit price.
  await page.getByRole("button", { name: "500 g $24.99" }).click()
  await page.getByRole("button", { name: `One more ${NAME}` }).click()
  await page.getByRole("contentinfo").scrollIntoViewIfNeeded()
  await expect(bar).toBeVisible()
  await expect(bar).toContainText(NAME)
  await expect(bar).toContainText("500 g × 2")
  await expectTapTargets(page, { within: "[data-sticky-buy]" })
  await expectNoHorizontalScroll(page)

  // The bar never hides the end of the page.
  const box = (await bar.boundingBox())!
  const language = page.getByRole("contentinfo").getByLabel("Language")
  await page.evaluate(() =>
    window.scrollTo(0, document.documentElement.scrollHeight)
  )
  const last = (await language.boundingBox())!
  expect(last.y + last.height).toBeLessThanOrEqual(box.y)

  await bar.getByRole("button", { name: "Add · $24.99" }).click()
  await expect(page.getByRole("link", { name: "Cart, 2 items" })).toBeVisible()
  await bar.getByRole("link", { name: "View cart" }).click()
  await expect(page).toHaveURL("/cart", { timeout: 15_000 })
  await expect(page.getByRole("list", { name: "Your cart" })).toContainText(
    "500 g"
  )
})

test("no bar on a desktop, or for a sold-out product", async ({
  page,
  isMobile,
}) => {
  if (!isMobile) {
    await page.goto(`/products/${PRODUCT.honey}`)
    await expect(
      page.getByRole("button", { name: "Add to cart" })
    ).toBeEnabled()
    await page.getByRole("contentinfo").scrollIntoViewIfNeeded()
    await expect(page.getByRole("region", { name: "Quick add" })).toHaveCount(0)
  }
  await page.goto(`/products/${PRODUCT.wax}`)
  await page.getByRole("contentinfo").scrollIntoViewIfNeeded()
  await expect(page.getByRole("region", { name: "Quick add" })).toHaveCount(0)
})
