import { expectThemeGrounds } from "../../src/theme"
import { expect, test } from "../../src/test"

test("shop paints the warm light and neutral dark grounds", async ({ page }) => {
  await expectThemeGrounds(page, "/")
})

test("prices read in gold ink on the product cards", async ({ page }) => {
  for (const [scheme, ink] of [
    ["light", "rgb(138, 97, 22)"],
    ["dark", "rgb(239, 201, 119)"],
  ] as const) {
    await page.emulateMedia({ colorScheme: scheme })
    await page.goto("/")
    await expect(page.locator(".text-gold-ink").first()).toHaveCSS("color", ink)
  }
})
