// P9-2: forced colours drop box-shadow rings, so focus must show as an outline there.
import { focusLook } from "../../src/focus"
import { expect, test } from "../../src/test"

test.use({ forcedColors: "active" })

test("P9-2: the login form shows keyboard focus in forced colours", async ({ page }) => {
  await page.goto("/login")
  for (const control of [page.getByLabel("Email"), page.getByRole("button", { name: "Log in" })]) {
    const look = await focusLook(control)
    expect(look.visible).toBe(true)
    expect(look.outline).not.toBe("none")
    expect(look.width).toBeGreaterThanOrEqual(2)
  }
})
