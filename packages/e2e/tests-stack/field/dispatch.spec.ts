import { cred, expect, test, urls } from "../../src/stack"
import { signIn } from "./field"

// The real API's permissions: ops once got 403 on Dispatch (it read supply-links and warehouses,
// which need workspace rights). The guard fails the test on any >= 400 response or console error.
test("ops open Dispatch: the seeded transfers, the dispatch form and the Buy harvest card, nothing refused", async ({ page, request }) => {
  await signIn(page, request, cred("field (RUAHA)", "ops"))
  await page.goto(`${urls.field}/ops/dispatch`)
  await expect(page.getByTestId("supply-transfer-row").first()).toBeVisible()
  await expect(page.getByTestId("dispatch-send")).toBeVisible()
  await expect(page.getByTestId("dispatch-link").locator("option")).not.toHaveCount(1)
  await expect(page.getByTestId("dispatch-warehouse").locator("option", { hasText: "Ilundo store" })).toHaveCount(1)
  await expect(page.getByTestId("batch-form")).toBeVisible()
})

// Bug: RUAHA's default currency is Euro, so Buy harvest prices a Tanzanian harvest in euros and
// farmers are credited in euros unless ops change it by hand.
test("Buy harvest in the Tanzanian programme starts in shillings", async ({ page, request }) => {
  test.fail(true, "RUAHA workspace default currency is EUR (demo seed / workspace settings), not TZS (e2e-field)")
  await signIn(page, request, cred("field (RUAHA)", "ops"))
  await page.goto(`${urls.field}/ops/dispatch`)
  const currency = page.getByTestId("batch-currency")
  await expect(currency.locator("option:checked")).toHaveText("Tanzanian shilling")
})
