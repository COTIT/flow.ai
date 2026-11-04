import type { Page } from "@playwright/test"

import { mockApi, type MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, role, villages } from "./api"
import { farmerMe } from "./farmer.api"
import { PERSON, PLOT, crops, documents, holdings, person, tourSeen } from "./officer-records.api"
import { emptyPassbook, fourEyes, noPayouts, overBalance, overLimit, paid, passbook, payouts, unavailable } from "./passbook.api"

const KEY = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/
const PAYOUTS = "/workspace/w1/farm/payouts"

async function asFarmer(page: Page, book: MockReply, paidOut: MockReply, farmer = farmerMe) {
  await seedSession(page)
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["farmer:u1"])))
  await mockApi(page, {
    "GET /farm/me": farmer,
    "GET /workspace/w1/farm/surveys/eligibility": noPayouts,
    "GET /workspace/w1/farm/passbook/mine": book,
    [`GET ${PAYOUTS}`]: paidOut,
  })
  await page.goto("/farm/passbook")
}

test("a farmer reads their balance, money lines and payouts", async ({ page }) => {
  await asFarmer(page, passbook, payouts)
  await expect(page.getByTestId("passbook-balance-TZS")).toContainText("5,000.00")
  const lines = page.getByTestId("passbook-line")
  await expect(lines).toHaveCount(2)
  await expect(lines.first()).toContainText("Payout")
  await expect(lines.nth(1)).toContainText("Sale credit")
  await expect(page.getByTestId("payout-row")).toContainText("NIDA")
  await expect(page.getByTestId("pay-out-TZS")).toHaveCount(0)
  await expectNoHorizontalScroll(page)
})

test("a farmer with nothing yet sees why", async ({ page }) => {
  await asFarmer(page, emptyPassbook, noPayouts)
  await expect(page.getByTestId("passbook-empty")).toBeVisible()
  await expect(page.getByTestId("payouts-empty")).toBeVisible()
  await expectNoHorizontalScroll(page)
})

/**
 * Staff on Neema's page; `answers` replies to each payout POST in turn (the last repeats), "drop" cuts
 * the connection. Returns the bodies sent.
 */
async function asStaff(page: Page, answers: (MockReply | "drop")[], { locale = "en" as "en" | "sw", book = passbook } = {}) {
  await seedSession(page)
  await tourSeen(page)
  await mockApi(page, {
    "GET /farm/me": me({ locale }),
    "GET /workspace/w1/farm/villages": villages,
    [`GET /workspace/w1/farm/persons/${PERSON}`]: { body: { data: person() } },
    [`GET /workspace/w1/farm/persons/${PERSON}/holdings`]: { body: holdings("Riverside farm") },
    [`GET /workspace/w1/farm/persons/${PERSON}/login-history`]: { body: { data: [] } },
    "GET /workspace/w1/farm/crops": crops,
    [`GET /workspace/w1/farm/plots/${PLOT}/documents`]: documents,
    [`GET /workspace/w1/farm/persons/${PERSON}/passbook`]: book,
  })
  const sent: Record<string, unknown>[] = []
  await page.route(`**/v1${PAYOUTS}**`, async (route) => {
    if (route.request().method() !== "POST") {
      return route.fulfill({ contentType: "application/json", body: JSON.stringify(payouts.body) })
    }
    sent.push(route.request().postDataJSON())
    const answer = answers[Math.min(sent.length, answers.length) - 1]!
    if (answer === "drop") return route.abort("internetdisconnected")
    await route.fulfill({ status: answer.status ?? 200, contentType: "application/json", body: JSON.stringify(answer.body) })
  })
  await page.goto(`/officer/people/${PERSON}`)
  return sent
}

async function fillPayout(page: Page, amount: string) {
  await page.getByTestId("pay-out-TZS").click()
  await page.getByTestId("payout-amount").fill(amount)
  await page.getByTestId("payout-id-type").selectOption("nida")
  await page.getByTestId("payout-name-confirmed").check()
}

test("staff pay a farmer out of the passbook in minor units under one client_ref", async ({ page }) => {
  const sent = await asStaff(page, [paid])
  await expect(page.getByTestId("person-passbook")).toContainText("5,000.00")
  await fillPayout(page, "2000")
  await expectNoHorizontalScroll(page)
  await page.getByTestId("confirm-dialog-confirm").click()
  await expect(page.getByTestId("payout-done")).toContainText("2,000.00")
  expect(sent).toHaveLength(1)
  expect(sent[0]).toMatchObject({ person_id: PERSON, amount: 200000, currency_id: "tzs", id_type: "nida", name_confirmed: true })
  expect(sent[0]!.client_ref).toMatch(KEY)
})

test.describe("refusals", () => {
  test.use({ allow: { responses: [{ url: `/v1${PAYOUTS}`, status: 422 }, { url: `/v1${PAYOUTS}`, status: 403 }] } })

  test("over the balance shows under the amount; a retry keeps the client_ref", async ({ page }) => {
    const sent = await asStaff(page, [overBalance, paid])
    await fillPayout(page, "9000")
    await page.getByTestId("confirm-dialog-confirm").click()
    await expect(page.getByTestId("payout-amount-error")).toContainText("greater than the balance")
    await page.getByTestId("payout-amount").fill("2000")
    await page.getByTestId("confirm-dialog-confirm").click()
    await expect(page.getByTestId("payout-done")).toBeVisible()
    expect(sent).toHaveLength(2)
    expect(sent[1]!.client_ref).toBe(sent[0]!.client_ref)
  })

  test("four eyes: the API's reason is shown in the user's words", async ({ page }) => {
    await asStaff(page, [fourEyes])
    await fillPayout(page, "2000")
    await page.getByTestId("confirm-dialog-confirm").click()
    await expect(page.getByTestId("payout-error")).toContainText("You registered this household, so another staff member must pay this farmer.")
    await expect(page.getByTestId("confirm-dialog")).toBeVisible()
  })
})

test.describe("the officer limit", () => {
  test.use({ allow: { responses: [{ url: `/v1${PAYOUTS}`, status: 403 }] } })

  test("above it the officer is told ops or admin must pay, and nothing is paid", async ({ page }) => {
    const sent = await asStaff(page, [overLimit])
    await fillPayout(page, "4000")
    await page.getByTestId("confirm-dialog-confirm").click()
    await expect(page.getByTestId("payout-error")).toContainText("above the officer limit: ops or admin must pay it")
    await expect(page.getByTestId("payout-done")).toHaveCount(0)
    await expect(page.getByTestId("person-passbook")).toContainText("5,000.00")
    expect(sent).toHaveLength(1)
  })

  // Bug (raw value): payout refusals are API English shown as is; a Swahili officer cannot read them.
  // dbMessages.ts translates voucher and registration messages, not farm_payouts/service.rs.
  test("a Swahili officer reads the refusal in Swahili", async ({ page }) => {
    await asStaff(page, [overLimit], { locale: "sw" })
    await page.getByTestId("pay-out-TZS").click()
    await page.getByTestId("payout-amount").fill("4000")
    await page.getByTestId("payout-id-type").selectOption("nida")
    await page.getByTestId("payout-name-confirmed").check()
    await page.getByTestId("confirm-dialog-confirm").click()
    await expect(page.getByTestId("payout-error")).toBeVisible()
    await expect(page.getByTestId("payout-error")).not.toContainText("ops or admin must pay it")
  })
})

test.describe("a payout with no network", () => {
  test.use({ allow: { failures: [`/v1${PAYOUTS}`] } })

  test("says No network; sending again goes under the same client_ref and pays once", async ({ page }) => {
    const sent = await asStaff(page, ["drop", paid])
    await fillPayout(page, "2000")
    await page.getByTestId("confirm-dialog-confirm").click()
    await expect(page.getByTestId("payout-error")).toContainText("No network")
    await expect(page.getByTestId("payout-done")).toHaveCount(0)
    await page.getByTestId("confirm-dialog-confirm").click()
    await expect(page.getByTestId("payout-done")).toContainText("2,000.00")
    expect(sent).toHaveLength(2)
    expect(sent[1]).toEqual(sent[0])
    await expectNoHorizontalScroll(page)
  })
})

test.describe("a passbook that fails to load", () => {
  test.use({ allow: { responses: [{ url: `/v1/workspace/w1/farm/persons/${PERSON}/passbook?page=1&page_size=100`, status: 503 }] } })

  test("on the person page shows the error with a retry and offers no Pay out", async ({ page }) => {
    await asStaff(page, [paid], { book: unavailable })
    const section = page.getByTestId("person-passbook")
    await expect(section.getByTestId("error-state")).toBeVisible()
    await expect(section.getByTestId("error-retry")).toBeVisible()
    await expect(page.getByTestId("pay-out-TZS")).toHaveCount(0)
  })
})

test.describe("a farmer's passbook with no network", () => {
  test.use({ allow: { failures: ["/v1/workspace/w1/farm/passbook/mine?page=1&page_size=100"] } })

  test("says No network, not empty; back online, retry shows the lines", async ({ page }) => {
    await asFarmer(page, passbook, payouts)
    await expect(page.getByTestId("passbook-line")).toHaveCount(2)
    // Registered after mockApi, so it answers first.
    let cut = true
    await page.route("**/v1/workspace/w1/farm/passbook/mine**", (route) => (cut ? route.abort("internetdisconnected") : route.fallback()))
    await page.reload()
    const error = page.getByTestId("my-passbook").getByTestId("error-state").first()
    await expect(error).toContainText("No network")
    await expect(page.getByTestId("passbook-empty")).toHaveCount(0)
    cut = false
    await error.getByTestId("error-retry").click()
    await expect(page.getByTestId("passbook-line")).toHaveCount(2)
  })
})

test("a Swahili farmer reads the passbook in Swahili", async ({ page }) => {
  await asFarmer(page, passbook, payouts, me({ name: "Neema Mwakalinga", email: null, phone: "0712345678", locale: "sw", memberships: [role("farmer")] }))
  const lines = page.getByTestId("passbook-line")
  await expect(lines.first()).toContainText("Malipo")
  await expect(lines.nth(1)).toContainText("Mapato ya mauzo")
  await expect(page.getByTestId("passbook-balance-TZS")).toContainText("5,000.00")
  await expectNoHorizontalScroll(page)
})
