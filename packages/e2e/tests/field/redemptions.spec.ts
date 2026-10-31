import type { Page } from "@playwright/test"

import { mockApi, type MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, role, villages } from "./api"
import { log, notOps, totals } from "./redemptions.api"

const RANGE = "from=2026-09-22&to=2026-09-29"

async function open(page: Page, totalsReply: MockReply) {
  await seedSession(page)
  // The first-visit guided tour would cover the screen; this user has seen the ops one.
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["ops:u1"])))
  await mockApi(page, {
    "GET /farm/me": me({ name: "Juma Ops", memberships: [role("ops", null)] }),
    "GET /workspace/w1/farm/villages": villages,
    "GET /workspace/w1/farm/redemptions/totals": totalsReply,
    "GET /workspace/w1/farm/redemptions": log,
  })
  await page.goto(`/ops/surveys/redemptions?${RANGE}`)
}

test("ops read the totals the API summed and every hand-over in the range", async ({ page }) => {
  const sent = page.waitForRequest("**/v1/workspace/w1/farm/redemptions/totals?*")
  await open(page, totals)
  expect(new URL((await sent).url()).search).toBe(`?${RANGE}`)

  const rows = page.getByTestId("redemption-total-row")
  await expect(rows).toHaveCount(2)
  await expect(rows.first()).toContainText("Asha Officer")
  await expect(rows.first()).toContainText("TZS 10,000.00")

  const handover = page.getByTestId("redemption-row")
  await expect(handover).toContainText("Mwakalinga household")
  await expect(handover).toContainText("TZS 5,000.00")
  await expect(handover).toContainText("NIDA card")
  await expectNoHorizontalScroll(page)
})

test.describe("a refused read", () => {
  test.use({
    allow: { responses: [{ url: `/v1/workspace/w1/farm/redemptions/totals?${RANGE}`, status: 403 }] },
  })

  test("shows the API's message", async ({ page }) => {
    await open(page, notOps)
    await expect(page.getByTestId("error-state")).toContainText("only ops or admin may read redemptions")
    await expectNoHorizontalScroll(page)
  })
})
