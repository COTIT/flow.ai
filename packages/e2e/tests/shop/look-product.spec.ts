import type { Locator } from "@playwright/test"

import { PRODUCT } from "../../mock/fixtures.mjs"
import {
  expect,
  expectNoHorizontalScroll,
  expectNoLayoutShift,
  expectNoMotion,
  expectTapTargets,
  test,
} from "../../src/test"

// Mock `ShopProduct` in docs/design/web-redesign/canvas: values below are copied from it.
const NAME = "Ilundo Wild Forest Honey"

const css = (l: Locator, prop: string) =>
  l.evaluate((el, p) => getComputedStyle(el).getPropertyValue(p), prop)

test("the product page is the bento from the mockup", async ({
  page,
  isMobile,
}) => {
  await page.goto(`/products/${PRODUCT.honey}`)
  await expect(
    page.getByRole("link", { name: "Back to the shop" })
  ).toBeVisible()

  const h1 = page.getByRole("heading", { level: 1, name: NAME })
  expect(await css(h1, "font-size")).toBe("28px")

  // Photo tile beside the buy tile on a desktop, above it on a phone.
  const photo = page.getByRole("img", { name: NAME }).first()
  await expect(photo).toBeVisible()
  expect(
    await photo.evaluate((img: HTMLImageElement) => img.naturalWidth)
  ).toBeGreaterThan(0)
  const photoBox = (await photo.boundingBox())!
  const titleBox = (await h1.boundingBox())!
  if (isMobile) expect(titleBox.y).toBeGreaterThan(photoBox.y + photoBox.height)
  else expect(titleBox.x).toBeGreaterThan(photoBox.x + photoBox.width)

  // Two photos: thumbnails switch the big one.
  const second = page.getByRole("button", { name: "Photo 2 of 2" })
  await expect(
    page.getByRole("button", { name: "Photo 1 of 2" })
  ).toHaveAttribute("aria-pressed", "true")
  await second.click()
  await expect(second).toHaveAttribute("aria-pressed", "true")
  await expect(photo).toHaveAttribute("src", /honey-2/)

  // Sizes are buttons with their price; the big price follows the chosen size.
  const sizes = page.getByRole("group", { name: "Size" })
  const small = sizes.getByRole("button", { name: "250 g $14.99" })
  const large = sizes.getByRole("button", { name: "500 g $24.99" })
  await expect(small).toHaveAttribute("aria-pressed", "true")
  expect(await css(large, "height")).toBe("56px")
  const price = page.getByText("$14.99", { exact: true }).first()
  expect(await css(price, "font-size")).toBe("32px")
  await large.click()
  await expect(large).toHaveAttribute("aria-pressed", "true")
  await expect(page.getByText("$24.99", { exact: true }).first()).toBeVisible()
  await expect(page.getByText("In stock")).toBeVisible()

  await page.getByRole("button", { name: `One more ${NAME}` }).click()
  await expect(page.getByLabel("Quantity")).toHaveText("2")
  const add = page.getByRole("button", { name: "Add to cart" })
  expect(await css(add, "height")).toBe("52px")
  await add.click()
  const added = page
    .locator('[data-variant="success"]')
    .filter({ hasText: "Added to your cart." })
  await expect(added.getByRole("link", { name: "View cart" })).toBeVisible()
  await expect(page.getByRole("link", { name: "Cart, 2 items" })).toBeVisible()
  await expect(
    page.getByText("Shipping and VAT are worked out at checkout.")
  ).toBeVisible()

  await expect(
    page.getByRole("heading", { level: 2, name: "About this product" })
  ).toBeVisible()
  await expect(
    page.getByText(
      "Harvested by the Ilundo beekeepers from traditional log hives."
    )
  ).toBeVisible()

  await expectNoHorizontalScroll(page)
  if (isMobile) await expectTapTargets(page)
  await expectNoLayoutShift(page)
})

test("a product without photos shows the honey placeholder", async ({
  page,
}) => {
  await page.goto(`/products/${PRODUCT.wax}`)
  await expect(
    page.getByRole("img", { name: "Photo coming soon" })
  ).toBeVisible()
  await expect(page.getByRole("button", { name: /Photo \d of/ })).toHaveCount(0)
  await expectNoHorizontalScroll(page)
})

test("with motion, the photo glares only under a mouse", async ({
  page,
  isMobile,
}) => {
  await page.goto(`/products/${PRODUCT.honey}`)
  await expect(page.locator("main [data-motion]")).toHaveCount(isMobile ? 0 : 1)
})

test.describe("with reduced motion", () => {
  test.use({ reducedMotion: "reduce" })

  test("nothing animates", async ({ page }) => {
    await page.goto(`/products/${PRODUCT.honey}`)
    await expect(page.getByRole("img", { name: NAME }).first()).toBeVisible()
    await expectNoMotion(page)
  })
})
