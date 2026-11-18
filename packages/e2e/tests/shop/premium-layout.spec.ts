import { PRODUCT, SHOP_CONNECTIONS } from "../../mock/fixtures.mjs"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"

test("premium layout is responsive with aligned product prices and stable FAQ toggles", async ({
  page,
  isMobile,
}) => {
  test.skip(isMobile, "This check sets five explicit viewport widths")
  test.setTimeout(75_000)

  for (const width of [320, 390, 768, 1440, 1920]) {
    await page.setViewportSize({ width, height: 900 })
    for (const path of [
      "/",
      "/shop",
      `/products/${PRODUCT.honey}`,
      `/products/${PRODUCT.basket}`,
      "/contact",
      "/faq",
    ]) {
      await page.goto(path)
      await expect(page.locator("body")).toHaveAttribute(
        "data-shop-theme",
        "afm"
      )
      await expectNoHorizontalScroll(page)
      if (path === "/shop") {
        const rows = await page
          .locator(".afm-product-card")
          .evaluateAll((cards) =>
            cards.map((card) => {
              const bounds = card.getBoundingClientRect()
              const price = card
              .querySelector(".afm-product-price > :first-child")!
                .getBoundingClientRect()
              return {
                top: bounds.top,
                bottom: bounds.bottom,
                price: price.bottom,
              }
            })
          )
        expect(rows.length).toBeGreaterThan(1)
        for (const row of rows) {
          for (const peer of rows.filter(
            (card) => Math.abs(card.top - row.top) < 1
          )) {
            expect(Math.abs(row.bottom - peer.bottom)).toBeLessThan(1)
            expect(Math.abs(row.price - peer.price)).toBeLessThan(1)
          }
        }
      }
      if (path === "/faq") {
        const summary = page.locator("main details summary").first()
        const bounds = await summary.boundingBox()
        await summary.focus()
        await summary.press("Enter")
        await expect(summary.locator("..")).toHaveAttribute("open", "")
        expect(await summary.boundingBox()).toEqual(bounds)
      }
    }
  }
})

test("the default theme and product hero stay outside tenant storefronts", async ({
  page,
}) => {
  await page.goto("/")
  await expect(page.locator("body")).toHaveAttribute("data-shop-theme", "afm")
  const feature = page.locator(".afm-hero-product")
  await expect(feature).toContainText("Ilundo Wild Forest Honey")
  await expect(feature).toContainText("$14.99")
  await expect(feature.getByRole("link").last()).toHaveAttribute(
    "href",
    `/products/${PRODUCT.honey}`
  )
  await page.goto(`/s/${SHOP_CONNECTIONS[0]}`)
  await expect(page.locator("body")).not.toHaveAttribute(
    "data-shop-theme",
    "afm"
  )
  await expect(page.locator(".afm-hero-product")).toHaveCount(0)
  await expect(page.getByRole("banner")).toContainText("First shop")
})
