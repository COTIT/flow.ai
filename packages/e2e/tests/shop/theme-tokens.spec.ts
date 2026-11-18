import { expect, test } from "../../src/test"

test("the AFM ground and price ink adapt to the system color scheme", async ({
  page,
}) => {
  for (const [scheme, ground, ink] of [
    ["light", "rgb(244, 240, 229)", "rgb(112, 80, 29)"],
    ["dark", "rgb(24, 36, 30)", "rgb(244, 210, 148)"],
  ] as const) {
    await page.emulateMedia({ colorScheme: scheme })
    await page.goto("/")
    await expect(page.locator("body")).toHaveCSS("background-color", ground)
    await expect(page.locator(".text-gold-ink").first()).toHaveCSS("color", ink)
  }
})
