import { expectInstallable } from "../../src/pwa"
import { test } from "../../src/test"

test("can be added to the home screen as 'Ruaha 360'", async ({ page }) => {
  await page.goto("/login")
  await expectInstallable(page, { name: "Ruaha 360", shortName: "Ruaha 360", startUrl: "/" })
})
