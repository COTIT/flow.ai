import { expectThemeGrounds } from "../../src/theme"
import { test } from "../../src/test"

test("flow paints the warm light and neutral dark grounds", async ({ page }) => {
  await expectThemeGrounds(page, "/login")
})
