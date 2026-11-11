// P9: keyboard and screen-reader focus. SIMULATED API + voice.
import { focusLook } from "../../src/focus"
import { expect, test } from "./storefront-api"
import { v } from "./voice"

test.beforeEach(({ api }) => void api)

const controls = (page: import("@playwright/test").Page) => ({
  button: page.getByRole("button", { name: v.textInstead, exact: true }),
  field: page.getByLabel(v.message),
})

test.describe("P9-2: forced colours (Windows High Contrast)", () => {
  test.use({ forcedColors: "active" })
  test("focus is drawn as an outline on buttons and fields", async ({ page }) => {
    await page.goto("/")
    for (const [name, control] of Object.entries(controls(page))) {
      const look = await focusLook(control)
      expect(look.visible, name).toBe(true)
      expect(look.outline, `${name}: no outline in forced colours`).not.toBe("none")
      expect(look.width, name).toBeGreaterThanOrEqual(2)
    }
  })
})

test("P9-2: the normal focus look is unchanged (ring, no outline)", async ({ page }) => {
  await page.goto("/")
  const look = await focusLook(controls(page).button)
  expect(look.visible).toBe(true)
  expect(look.outline).toBe("none")
  expect(look.ring).not.toBe("none")
})
