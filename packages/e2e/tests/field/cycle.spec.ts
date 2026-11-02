import type { Page } from "@playwright/test"

import { mockApi } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, villages } from "./api"
import { CYCLE, FIRST, PLOT, after, before, crops, cycle, page as listPage, plot } from "./cycle.api"

/** Opens the cycle; the harvest series answers `before` until a revision is posted, then `after`. */
async function open(page: Page, revision: { status: number; body: unknown }) {
  await seedSession(page)
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["officer:u1"])))
  await mockApi(page, {
    "GET /farm/me": me(),
    "GET /workspace/w1/farm/villages": villages,
    [`GET /workspace/w1/farm/crop-cycles/${CYCLE}`]: cycle,
    [`GET /workspace/w1/farm/plots/${PLOT}`]: plot,
    "GET /workspace/w1/farm/crops": crops,
  })
  const posted: unknown[] = []
  await page.route(`**/v1/workspace/w1/farm/crop-cycles/${CYCLE}/harvest-reports*`, async (route) => {
    const request = route.request()
    if (request.method() === "POST") {
      posted.push(request.postDataJSON())
      await route.fulfill({ status: revision.status, contentType: "application/json", body: JSON.stringify(revision.body) })
      return
    }
    const series = posted.length && revision.status < 400 ? after : before
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(listPage(series)) })
  })
  await page.goto(`/officer/cycles/${CYCLE}`)
  return posted
}

async function revise(page: Page, kg: string) {
  await page.getByTestId(`edit-harvest_report-${FIRST}`).click()
  await page.getByTestId("edit-quantity_kg").fill(kg)
  await page.getByTestId("officer-edit-save").click()
}

test("an officer reads the harvest series and a revision appears beside the figure it replaced", async ({ page }) => {
  const posted = await open(page, { status: 201, body: { data: after[0] } })

  await expect(page.getByRole("heading", { level: 1 })).toContainText("Maize · Riverside")
  await expect(page.getByTestId("cycle-harvest")).toHaveCount(1)
  await expect(page.getByTestId("cycle-harvest").first()).toContainText("3,200.00 kg")

  await revise(page, "4100")

  await expect(page.getByTestId("cycle-harvest")).toHaveCount(2)
  await expect(page.getByTestId("cycle-harvest").first()).toContainText("4,100.00 kg")
  await expect(page.getByTestId("cycle-harvest").nth(1)).toContainText("superseded")
  expect(posted).toEqual([
    // With the edit draft's reference, so a resend after a lost reply files the figure once.
    { kind: "expected", quantity_kg: "4100", source: "field_verified", confidence: "medium", reported_for: null, client_ref: expect.stringMatching(/^[0-9a-f-]{36}$/) },
  ])
  await expectNoHorizontalScroll(page)
})

test.describe("a refused revision", () => {
  test.use({
    allow: { responses: [{ url: `/v1/workspace/w1/farm/crop-cycles/${CYCLE}/harvest-reports`, status: 422 }] },
  })

  test("shows the API's message in the dialog, at its field", async ({ page }) => {
    await open(page, {
      status: 422,
      body: { message: "invalid", errors: { quantity_kg: ["The quantity kg must be at most 9999999999.99."] } },
    })
    await revise(page, "4100")

    await expect(page.getByTestId("edit-quantity_kg-error")).toHaveText("The quantity kg must be at most 9999999999.99.")
    await expect(page.getByTestId("officer-edit-error")).toHaveCount(0)
    await expect(page.getByTestId("cycle-harvest")).toHaveCount(1)
    await expectNoHorizontalScroll(page)
  })
})
