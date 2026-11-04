import { mockApi } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoRawValues, test } from "../../src/test"
import { me, role, villages } from "./api"
import { crops, currencies, dispatchOptions, lotStock, transfers } from "./supply.api"

// Dispatch shows quantities with their unit and every state in words, never an API code,
// a bare decimal or an id. Lot codes are legit codes.
test("dispatch: quantities in kg, states in words", async ({ page }) => {
  await seedSession(page)
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["ops:u1"])))
  await mockApi(page, {
    "GET /farm/me": me({ name: "Juma Ops", memberships: [role("ops", null)] }),
    "GET /workspace/w1/farm/villages": villages,
    "GET /workspace/w1/supply-transfers": transfers,
    "GET /workspace/w1/farm/dispatch/options": dispatchOptions,
    "GET /workspace/w1/lots/lot1/stock": lotStock,
    "GET /workspace/w1/farm/crops": crops,
    "GET /workspace/w1/currencies": currencies,
  })
  await page.goto("/ops/dispatch")
  await expect(page.getByTestId("supply-transfer-row").first()).toContainText("kg")
  await expectNoRawValues(page)
})
