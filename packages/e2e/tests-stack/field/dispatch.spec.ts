import { cred, expect, test, urls } from "../../src/stack"
import { expectNoRawValues } from "../../src/test"
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
  await expectNoRawValues(page)
})

// RUAHA's default currency is TZS (FB1), so Buy harvest prices and credits farmers in shillings.
test("Buy harvest in the Tanzanian programme starts in shillings", async ({ page, request }) => {
  await signIn(page, request, cred("field (RUAHA)", "ops"))
  await page.goto(`${urls.field}/ops/dispatch`)
  const currency = page.getByTestId("batch-currency")
  await expect(currency.locator("option:checked")).toHaveText("Tanzanian shilling")
})
