import { expectThemeGrounds } from "../../src/theme"
import { test } from "./fake-till"

test("till paints the warm light and neutral dark grounds", async ({ page }) => {
  await expectThemeGrounds(page, "/setup")
})
