import { cred, expect, loginField, test, urls } from "../src/stack"

// The real API's permissions: an ops user once got 403 on Dispatch. The guard fails the test on
// any >= 400 response or console error, so a refused call shows up here, not in a manual test.
test("ops logs into Field and Dispatch loads without errors", async ({ page }) => {
  test.fail(
    true,
    "ops gets 403 on GET supply-links and warehouses at Dispatch (dispatch fix on session api-gaps branch, not on main yet)"
  )
  await loginField(page, cred("field (RUAHA)", "ops"))
  await page.goto(`${urls.field}/ops/dispatch`)
  await expect(page.getByTestId("supply-transfer-row").first()).toBeVisible()
  await expect(page.getByTestId("dispatch-send")).toBeVisible()
})
