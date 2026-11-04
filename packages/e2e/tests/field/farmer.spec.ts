import type { Page } from "@playwright/test"

import { mockApi, type MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import {
  BLOCKED,
  CODE,
  SURVEY,
  VOUCHER,
  eligibilityAfter,
  eligibilityBefore,
  expiredVouchers,
  farmerMe,
  myVouchers,
  noVouchers,
  records,
  submitted,
  surveys,
  timeline,
  voidedVouchers,
} from "./farmer.api"
import { noOpportunities } from "./market.api"
import { emptyPassbook, passbook } from "./passbook.api"
import { requestList } from "./requests.api"

const RESPONSES = `/v1/workspace/w1/farm/surveys/${SURVEY}/responses`

/**
 * Signed in as a farmer (phone, no email). Each POST to the survey answers with the next reply;
 * once one succeeds, eligibility and vouchers/mine answer as the API would after the save.
 * Returns the bodies the app posted.
 */
async function open(
  page: Page,
  path: string,
  replies: { status: number; body: unknown }[] = [],
  extra: Record<string, MockReply> = {},
) {
  await seedSession(page)
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["farmer:u1"])))
  const routes: Record<string, MockReply> = {
    "GET /farm/me": farmerMe,
    ...records,
    ...surveys,
    "GET /workspace/w1/farm/surveys/eligibility": eligibilityBefore,
    "GET /workspace/w1/farm/passbook/mine": emptyPassbook,
    "GET /workspace/w1/farm/vouchers/mine": noVouchers,
    [`GET /workspace/w1/farm/vouchers/${VOUCHER}/timeline`]: timeline,
    // The home's latest-request card (the e2e build has the resources feature on).
    "GET /workspace/w1/farm/pue-requests": requestList(),
    "GET /workspace/w1/farm/opportunities": noOpportunities,
    ...extra,
  }
  await mockApi(page, routes)
  const posted: unknown[] = []
  await page.route(`**${RESPONSES}`, async (route) => {
    posted.push(route.request().postDataJSON())
    const reply = replies[posted.length - 1] ?? { status: 500, body: { message: "unexpected second submit" } }
    if (reply.status < 300) {
      routes["GET /workspace/w1/farm/surveys/eligibility"] = eligibilityAfter
      routes["GET /workspace/w1/farm/vouchers/mine"] = myVouchers
    }
    await route.fulfill({ status: reply.status, contentType: "application/json", body: JSON.stringify(reply.body) })
  })
  await page.goto(path)
  return posted
}

async function answer(page: Page) {
  await page.getByRole("radio", { name: "Yes" }).check()
  await page.getByRole("radio", { name: "In a crib" }).check()
  await page.getByLabel(/How many bags did you store/).fill("12")
}

async function expectVoucher(page: Page) {
  const card = page.getByTestId("voucher-card")
  await expect(card.getByTestId("voucher-code")).toHaveText("K7QXM-2PA9D")
  await expect(card.getByTestId("voucher-amount")).toHaveText("TZS 5,000.00")
  // The QR carries the prefixed code and nothing about the household.
  await expect(card.getByTestId("voucher-qr")).toHaveAttribute("data-payload", `R360V:${CODE}`)
  await expect(card.getByRole("img", { name: /K7QXM-2PA9D/ })).toBeVisible()
}

test("the farmer's home shows the farm summary and the new survey", async ({ page }) => {
  await open(page, "/farm")

  await expect(page.getByTestId("farm-home-summary")).toContainText("Riverside farm")
  await expect(page.getByTestId("summary-plots")).toHaveText("1")
  await expect(page.getByTestId("summary-cycles")).toHaveText("1")
  await expect(page.getByTestId("farm-home-surveys")).toContainText("1 new survey")
  // Opportunities are on the API: none yet is stated, not hidden.
  await expect(page.getByTestId("farm-home-opportunities")).toBeVisible()
  await expect(page.getByTestId("farm-home-no-requests")).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test("the farmer's home shows their passbook balance and opens the passbook", async ({ page }) => {
  await open(page, "/farm", [], {
    "GET /workspace/w1/farm/passbook/mine": passbook,
    "GET /workspace/w1/farm/payouts": { body: { data: [], meta: { current_page: 1, from: null, to: null, last_page: 1, per_page: 100, total: 0 } } },
  })
  await expect(page.getByTestId("passbook-home-TZS")).toContainText("5,000.00")
  await page.getByTestId("farm-home-passbook").getByRole("link").click()
  await expect(page).toHaveURL(/\/farm\/passbook$/)
  await expect(page.getByTestId("passbook-balance-TZS")).toContainText("5,000.00")
  await expectNoHorizontalScroll(page)
})

test("my farm shows the household, farm, plot and crop with its current expected harvest", async ({ page }) => {
  await open(page, "/farm/my-farm")

  const farm = page.getByTestId("farm-card")
  await expect(farm).toContainText("Riverside farm")
  await expect(farm.getByTestId("farm-household")).toHaveText("Mwakalinga household")
  await expect(farm.getByTestId("plot-card")).toContainText("Riverside")
  await expect(farm.getByTestId("plot-card")).toContainText("1.8000 ha")
  const cycle = farm.getByTestId("cycle-card")
  await expect(cycle).toContainText("Maize")
  await expect(cycle).toContainText("4,100.00 kg")
  await expectNoHorizontalScroll(page)
})

test("the survey list offers the eligible survey and gives the API's reason for the other", async ({ page }) => {
  await open(page, "/farm/surveys")

  const eligible = page.locator(`[data-testid="survey-card"][data-survey-id="${SURVEY}"]`)
  await expect(eligible).toHaveAttribute("data-state", "new")
  await expect(eligible.getByTestId("survey-open")).toBeVisible()
  const blocked = page.locator(`[data-testid="survey-card"][data-survey-id="${BLOCKED}"]`)
  await expect(blocked).toHaveAttribute("data-state", "unavailable")
  await expect(blocked.getByTestId("survey-reason")).toHaveText("Your household has not been verified yet.")
  await expect(blocked.getByTestId("survey-open")).toHaveCount(0)
  await expectNoHorizontalScroll(page)
})

test("answering a survey posts once with a client_ref and shows the voucher", async ({ page }) => {
  const posted = await open(page, `/farm/surveys/${SURVEY}`, [{ status: 201, body: submitted(false) }])

  await answer(page)
  await page.getByTestId("survey-submit").click()

  await expectVoucher(page)
  expect(posted).toHaveLength(1)
  expect(posted[0]).toEqual({
    answers: { "q-yn": true, "q-sc": "crib", "q-num": "12" },
    client_ref: expect.stringMatching(/^[0-9a-f-]{36}$/),
  })
  await expectNoHorizontalScroll(page)
})

test("a voucher ops voided says so, with their reason and no QR code", async ({ page }) => {
  await open(page, `/farm/surveys/${SURVEY}`, [], {
    "GET /workspace/w1/farm/surveys/eligibility": eligibilityAfter,
    "GET /workspace/w1/farm/vouchers/mine": voidedVouchers,
  })

  const card = page.getByTestId("voucher-card")
  await expect(card).toHaveAttribute("data-status", "void")
  await expect(card.getByTestId("voucher-validity")).toHaveText("Cancelled: Duplicate household")
  await expect(card.getByTestId("voucher-qr")).toHaveCount(0)
  await expectNoHorizontalScroll(page)
})

test("a voucher left past its expiry says when it expired, and shows no QR code", async ({ page }) => {
  await open(page, `/farm/surveys/${SURVEY}`, [], {
    "GET /workspace/w1/farm/surveys/eligibility": eligibilityAfter,
    "GET /workspace/w1/farm/vouchers/mine": expiredVouchers,
  })
  const card = page.getByTestId("voucher-card")
  await expect(card).toHaveAttribute("data-status", "expired")
  await expect(card).toContainText("Expired")
  await expect(card.getByTestId("voucher-validity")).toContainText(/This voucher expired on 21 Oct\w* 2026/)
  await expect(card.getByTestId("voucher-qr")).toHaveCount(0)
  await expectNoHorizontalScroll(page)
})

test.describe("a submit whose reply was lost", () => {
  test.use({ allow: { responses: [{ url: RESPONSES, status: 503 }] } })

  test("is retried with the same client_ref and the replayed reply shows the same voucher", async ({ page }) => {
    const posted = await open(page, `/farm/surveys/${SURVEY}`, [
      { status: 503, body: { message: "service unavailable" } },
      { status: 200, body: submitted(true) },
    ])

    await answer(page)
    await page.getByTestId("survey-submit").click()
    await expect(page.getByTestId("survey-submit-error")).toBeVisible()
    await page.getByTestId("survey-submit").click()

    await expectVoucher(page)
    expect(posted).toHaveLength(2)
    expect((posted[1] as { client_ref: string }).client_ref).toBe((posted[0] as { client_ref: string }).client_ref)
  })
})
