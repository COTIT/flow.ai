import { expectThemeGrounds } from "../../src/theme"
import { test } from "../../src/test"

test("landing paints the warm light and neutral dark grounds", async ({ page }) => {
  await expectThemeGrounds(page, "/", ".landing")
})
