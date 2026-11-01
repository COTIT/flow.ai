import type { Page } from "@playwright/test"

import { mockApi, type MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, role, villages } from "./api"
import { currencies } from "./equipment.api"
import { farmerHome, farmerMe } from "./farmer.api"
import {
  DEMAND,
  OPPORTUNITY,
  available,
  buyers,
  crops,
  demand,
  demands,
  farmerOpportunities,
  iringa,
  matches,
  mbeya,
  one,
  opportunity,
  overCommitted,
} from "./market.api"
import { officerHome } from "./officer-records.api"
import { requestList } from "./requests.api"

const W = "/workspace/w1/farm"
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/

type Sent = { key: string; body: unknown }
type Who = "farmer" | "ops" | "field_officer"

/**
 * Signed in as `who`. Every call is recorded (`calls`), writes with their body (`sent`); `effects`
 * run before a write is answered so the reads that follow see the API's new state.
 */
async function open(
  page: Page,
  who: Who,
  path: string,
  { extra = {}, effects = {} }: {
    extra?: Record<string, MockReply>
    effects?: Record<string, (routes: Record<string, MockReply>) => void>
  } = {},
) {
  await seedSession(page)
  const surface = who === "field_officer" ? "officer" : who
  await page.addInitScript((s) => localStorage.setItem("ruaha360:tours-seen", JSON.stringify([`${s}:u1`])), surface)
  const routes: Record<string, MockReply> = {
    "GET /farm/me": who === "farmer" ? farmerMe : me({ name: "Omari Ops", memberships: [who === "ops" ? role("ops", null) : role("field_officer")] }),
    "GET /workspace/w1/farm/villages": villages,
    "GET /workspace/w1/currencies": currencies,
    ...(who === "farmer" ? { ...farmerHome, [`GET ${W}/pue-requests`]: requestList() } : {}),
    ...(who === "field_officer" ? officerHome : {}),
    ...extra,
  }
  await mockApi(page, routes)
  const calls: string[] = []
  const sent: Sent[] = []
  await page.route("**/v1/**", async (route) => {
    const request = route.request()
    const key = `${request.method()} ${new URL(request.url()).pathname.replace(/^\/v1/, "")}`
    calls.push(key)
    if (request.method() !== "GET") sent.push({ key, body: request.postDataJSON() })
    effects[key]?.(routes)
    await route.fallback()
  })
  await page.goto(path)
  return { calls, sent }
}

test("a farmer sees the opportunity their harvest is in: their share, the total, only their line", async ({ page }) => {
  await open(page, "farmer", "/farm", { extra: { [`GET ${W}/opportunities`]: farmerOpportunities } })

  // The home card counts it and links to the list.
  await expect(page.getByTestId("farm-home-opportunities")).toContainText("1")
  await page.getByTestId("farm-home-opportunities-link").click()

  const row = page.getByTestId("farmer-opportunity")
  await expect(row).toHaveCount(1)
  await expect(row).toContainText("Maize")
  await expect(row.getByTestId("my-contribution")).toHaveText("4,100.00 kg")
  await expect(row.getByTestId("offered-total")).toHaveText("6,400.00 kg")
  // Another farmer's line is never sent to this farmer, and nothing names one.
  await expect(page.getByText("Juma Kalinga")).toHaveCount(0)
  await expect(page.getByTestId("farmer-opportunities-note")).toContainText(/not a sale/i)
  await expectNoHorizontalScroll(page)
})

test("ops adds a buyer with its draft id", async ({ page }) => {
  const { sent } = await open(page, "ops", "/ops/buyers", {
    extra: {
      [`GET ${W}/buyers`]: buyers(iringa),
      [`POST ${W}/buyers`]: { status: 201, body: { data: mbeya } },
    },
    effects: { [`POST ${W}/buyers`]: (routes) => { routes[`GET ${W}/buyers`] = buyers(iringa, mbeya) } },
  })

  await expect(page.getByTestId("buyer-row")).toHaveCount(1)
  await page.getByTestId("buyer-create-open").click()
  await page.getByTestId("buyer-name").fill("  Mbeya Millers ")
  await page.getByTestId("buyer-channel").click()
  await page.getByRole("option", { name: "AFM" }).click()
  await page.getByTestId("buyer-create-submit").click()

  await expect(page.getByTestId("buyer-row")).toHaveCount(2)
  expect(sent.find((s) => s.key === `POST ${W}/buyers`)!.body).toEqual({
    id: expect.stringMatching(UUID),
    name: "Mbeya Millers",
    channel: "afm",
    contact_note: null,
  })
  await expectNoHorizontalScroll(page)
})

test("ops records a demand: kg as text, the price in minor units, with its draft id", async ({ page }) => {
  const { sent } = await open(page, "ops", "/ops/demand", {
    extra: {
      [`GET ${W}/buyer-demands`]: demands(),
      [`GET ${W}/buyers`]: buyers(iringa),
      [`GET ${W}/crops`]: crops,
      [`POST ${W}/buyer-demands`]: { status: 201, body: { data: demand } },
    },
    effects: { [`POST ${W}/buyer-demands`]: (routes) => { routes[`GET ${W}/buyer-demands`] = demands(demand) } },
  })

  await page.getByTestId("demand-create-open").click()
  await page.getByTestId("demand-buyer").click()
  await page.getByRole("option", { name: "Iringa Grain" }).click()
  await page.getByTestId("demand-crop").click()
  await page.getByRole("option", { name: "Maize" }).click()
  await page.getByTestId("demand-quantity").fill("9000")
  await page.getByTestId("demand-price").fill("780")
  await page.getByTestId("demand-window-start").fill("2026-09-01")
  await page.getByTestId("demand-window-end").fill("2026-10-31")
  await page.getByTestId("demand-delivery").fill("Ilundo shed")
  await page.getByTestId("demand-create-submit").click()

  const row = page.getByTestId("demand-row")
  await expect(row).toHaveCount(1)
  await expect(row).toContainText("9,000.00 kg")
  await expect(row).toContainText("780.00")
  expect(sent.find((s) => s.key === `POST ${W}/buyer-demands`)!.body).toEqual({
    id: expect.stringMatching(UUID),
    buyer_id: "b1",
    crop_id: "c1",
    quantity_kg: "9000",
    window_start: "2026-09-01",
    window_end: "2026-10-31",
    delivery_point: "Ilundo shed",
    quality_note: null,
    indicative_price_per_kg_minor: 78000,
    currency_id: "c-tzs",
  })
  await expectNoHorizontalScroll(page)
})

test("ops opens a demand's comparison, then its opportunity with every supply line", async ({ page }) => {
  await open(page, "ops", `/ops/demand/${DEMAND}`, {
    extra: {
      [`GET ${W}/buyer-demands/${DEMAND}`]: one(demand),
      [`GET ${W}/buyer-demands/${DEMAND}/matches`]: matches,
      [`GET ${W}/opportunities/${OPPORTUNITY}`]: one(opportunity),
      [`GET ${W}/harvest-available`]: available,
    },
  })

  const match = page.getByTestId("match-row-v1")
  await expect(match).toContainText("Ilundo")
  await expect(match).toContainText("5,600.00 kg")
  await expect(match).toContainText("62.2%")
  await expectNoHorizontalScroll(page)

  await match.getByTestId("opportunity-link").getByRole("link").click()
  await expect(page).toHaveURL(new RegExp(`/ops/opportunities/${OPPORTUNITY}$`))
  await expect(page.getByTestId("offered-total")).toHaveText("6,400.00 kg")
  const lines = page.getByTestId("supply-row")
  await expect(lines).toHaveCount(2)
  await expect(lines.first()).toContainText("Neema Mwakalinga")
  await expect(lines.first().getByTestId("supply-attached-by")).toContainText("Omari Ops")
  await expectNoHorizontalScroll(page)
})

test.describe("attaching supply", () => {
  test.use({ allow: { responses: [{ url: `/v1${W}/opportunities/${OPPORTUNITY}/supply`, status: 422 }] } })

  test("asks first (cancel sends nothing), then shows the API's refusal as written", async ({ page }) => {
    const { sent } = await open(page, "ops", `/ops/opportunities/${OPPORTUNITY}`, {
      extra: {
        [`GET ${W}/opportunities/${OPPORTUNITY}`]: one(opportunity),
        [`GET ${W}/harvest-available`]: available,
        [`POST ${W}/opportunities/${OPPORTUNITY}/supply`]: overCommitted,
      },
    })

    await page.getByTestId("attach-harvest").click()
    await page.getByRole("option", { name: /Joseph Kalinga/ }).click()
    await page.getByTestId("attach-kg").fill("4500")
    await page.getByTestId("attach-submit").click()
    await expect(page.getByTestId("confirm-dialog")).toContainText("4,500.00 kg from Joseph Kalinga")
    await page.getByTestId("confirm-dialog-cancel").click()
    await expect(page.getByTestId("confirm-dialog")).toHaveCount(0)
    expect(sent).toHaveLength(0)

    await page.getByTestId("attach-submit").click()
    await page.getByTestId("confirm-dialog-confirm").click()
    await expect(page.getByTestId("attach-error")).toContainText(
      "over-commitment: 5000.00 kg available, 1000.00 kg already committed, 4500.00 kg requested",
    )
    expect(sent).toEqual([
      { key: `POST ${W}/opportunities/${OPPORTUNITY}/supply`, body: { id: expect.stringMatching(UUID), harvest_report_id: "h9", contributed_kg: "4500" } },
    ])
    await expectNoHorizontalScroll(page)
  })
})

// Matches are ops/admin only: a field officer never reaches the comparison and it is never asked for.
test("a field officer gets no matches section", async ({ page }) => {
  const { calls } = await open(page, "field_officer", `/ops/demand/${DEMAND}`)

  await expect(page).toHaveURL(/\/officer$/)
  await expect(page.getByTestId("demand-matches")).toHaveCount(0)
  expect(calls.filter((c) => c.includes("/matches") || c.includes("/buyer-demands"))).toEqual([])
  await expectNoHorizontalScroll(page)
})
