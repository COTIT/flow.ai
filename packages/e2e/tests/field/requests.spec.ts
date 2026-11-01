import type { Page } from "@playwright/test"

import { mockApi, type MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, role, villages } from "./api"
import { MILL, mill, one as oneMachine } from "./equipment.api"
import { eligibilityBefore, farmerMe } from "./farmer.api"
import { REQUEST, energy, estimatePreview, noteRequired, one, pueRequest, requestList } from "./requests.api"

const REQUESTS = "/workspace/w1/farm/pue-requests"
const TRANSITIONS = `${REQUESTS}/${REQUEST}/transitions`
const ESTIMATE = `${REQUESTS}/estimate`
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/

type Sent = { key: string; body: unknown }
type Effects = Record<string, (routes: Record<string, MockReply>, body: unknown) => void>

/** Signed in as `who`; writes are recorded, and `effects` set the reply (and later reads) before it is answered. */
async function open(page: Page, who: "farmer" | "ops", path: string, routes: Record<string, MockReply>, effects: Effects = {}) {
  await seedSession(page)
  await page.addInitScript((tour) => localStorage.setItem("ruaha360:tours-seen", JSON.stringify([tour])), `${who}:u1`)
  const table: Record<string, MockReply> = {
    ...(who === "farmer"
      ? { "GET /farm/me": farmerMe, "GET /workspace/w1/farm/surveys/eligibility": eligibilityBefore }
      : { "GET /farm/me": me({ name: "Omari Ops", memberships: [role("ops", null)] }) }),
    "GET /workspace/w1/farm/villages": villages,
    ...routes,
  }
  await mockApi(page, table)
  const sent: Sent[] = []
  await page.route("**/v1/**", async (route) => {
    const request = route.request()
    const key = `${request.method()} ${new URL(request.url()).pathname.replace(/^\/v1/, "")}`
    const body = request.method() === "GET" ? undefined : request.postDataJSON()
    if (body !== undefined) sent.push({ key, body })
    effects[key]?.(table, body)
    await route.fallback()
  })
  await page.goto(path)
  return sent
}

test("a farmer files a request from a machine, sees it listed and withdraws it", async ({ page }) => {
  const filed = pueRequest()
  const sent = await open(
    page,
    "farmer",
    `/farm/equipment/${MILL}`,
    {
      [`GET /workspace/w1/farm/equipment/${MILL}`]: oneMachine(mill),
      [`POST ${REQUESTS}`]: one(filed, 201),
      [`GET ${REQUESTS}`]: requestList(),
    },
    {
      // The preview answers for the figures it was sent.
      [`POST ${ESTIMATE}`]: (routes, body) => {
        routes[`POST ${ESTIMATE}`] = estimatePreview(body as Parameters<typeof estimatePreview>[0])
      },
      [`POST ${REQUESTS}`]: (routes) => {
        routes[`GET ${REQUESTS}`] = requestList(filed)
        routes[`GET ${REQUESTS}/${REQUEST}`] = one(filed)
      },
      [`POST ${TRANSITIONS}`]: (routes) => {
        const withdrawn = pueRequest({ status: "withdrawn" })
        routes[`POST ${TRANSITIONS}`] = one(withdrawn)
        routes[`GET ${REQUESTS}/${REQUEST}`] = one(withdrawn)
        routes[`GET ${REQUESTS}`] = requestList(withdrawn)
      },
    },
  )

  // Prefilled from the machine's typical use; the estimate is the API's preview, asked as the farmer types.
  await expect(page.getByTestId("request-hours")).toHaveValue("6")
  await expect(page.getByTestId("estimate-kwh-day")).toHaveText("90.000 kWh")
  await page.getByTestId("request-hours").fill("8")
  await expect(page.getByTestId("estimate-kwh-day")).toHaveText("120.000 kWh")
  await expect(page.getByTestId("estimate-kwh-week")).toHaveText("600.000 kWh")
  expect(sent.filter((s) => s.key === `POST ${ESTIMATE}`).at(-1)!.body).toEqual({
    equipment_id: MILL,
    quantity: 1,
    hours_per_day: "8",
    days_per_week: "5",
  })
  // A preview writes nothing: no request is filed until submit.
  expect(sent.filter((s) => s.key === `POST ${REQUESTS}`)).toHaveLength(0)
  await page.getByTestId("equipment-acquisition-buy").check()
  await page.getByTestId("request-purpose").fill("Milling for the co-op")
  await expectNoHorizontalScroll(page)
  await page.getByTestId("request-submit").click()

  await expect(page.getByTestId("request-success")).toBeVisible()
  const post = sent.find((s) => s.key === `POST ${REQUESTS}`)!.body as Record<string, unknown>
  // A client id made when the form opened; a farmer files for themselves, so no person is sent.
  expect(post).toEqual({
    id: expect.stringMatching(UUID),
    equipment_id: MILL,
    acquisition: "buy",
    quantity: 1,
    hours_per_day: "8",
    days_per_week: "5",
    purpose: "Milling for the co-op",
    status: "submitted",
  })
  expect(post).not.toHaveProperty("person_id")

  await page.goto("/farm/requests")
  const row = page.getByTestId("my-request-row")
  await expect(row).toHaveCount(1)
  await expect(row).toHaveAttribute("data-status", "submitted")
  await expect(row).toContainText("Maize mill")
  await expectNoHorizontalScroll(page)

  await row.click()
  await expect(page.getByTestId("stored-estimate-kwh-week")).toHaveText("450.000 kWh")
  await page.getByTestId("request-action-withdraw").click()

  await expect(page.getByTestId("request-detail").getByTestId("status-pill").first()).toHaveAttribute("data-status", "withdrawn")
  await expect(page.getByTestId("request-action-withdraw")).toHaveCount(0)
  expect(sent.filter((s) => s.key === `POST ${TRANSITIONS}`).map((s) => s.body)).toEqual([{ action: "withdraw" }])
  await expectNoHorizontalScroll(page)
})

test("ops starts a review and approves a request from the pipeline", async ({ page }) => {
  const sent = await open(
    page,
    "ops",
    "/ops/requests",
    {
      [`GET ${REQUESTS}`]: requestList(pueRequest()),
      [`GET ${REQUESTS}/${REQUEST}`]: one(pueRequest({ farm_id: "f1", farm: { id: "f1", label: "Kipande" } })),
      "GET /workspace/w1/farm/villages/v1/energy": energy,
    },
    {
      [`POST ${TRANSITIONS}`]: (routes, body) => {
        const status = (body as { action: string }).action === "start_review" ? "under_review" : "approved"
        const moved = pueRequest({ status, decision_note: status === "approved" ? "Fits the headroom" : null })
        routes[`POST ${TRANSITIONS}`] = one(moved)
        routes[`GET ${REQUESTS}/${REQUEST}`] = one(moved)
      },
    },
  )

  const row = page.getByTestId("request-row")
  await expect(row).toHaveCount(1)
  await expect(row).toContainText("Neema Mwakalinga")
  await expect(row).toContainText("Ilundo")
  await expectNoHorizontalScroll(page)
  await row.click()

  await expect(page.getByTestId("review-headroom")).toHaveText("500.000 kW")
  // The farm is named by the API.
  await expect(page.getByTestId("request-review")).toContainText("Kipande")
  await page.getByTestId("action-start_review").click()
  await page.getByTestId("decision-note").fill("Fits the headroom")
  // Approval cannot be undone: the dialog asks first, and going back sends nothing.
  await page.getByTestId("action-approve").click()
  await page.getByTestId("confirm-dialog-cancel").click()
  await expect(page.getByTestId("confirm-dialog")).toHaveCount(0)
  expect(sent.filter((s) => s.key === `POST ${TRANSITIONS}`)).toHaveLength(1)
  await page.getByTestId("action-approve").click()
  await page.getByTestId("confirm-dialog-confirm").click()

  await expect(page.getByTestId("action-approve")).toHaveCount(0)
  await expect(page.getByTestId("request-review")).toContainText("Fits the headroom")
  expect(sent.filter((s) => s.key === `POST ${TRANSITIONS}`).map((s) => s.body)).toEqual([
    { action: "start_review" },
    { action: "approve", note: "Fits the headroom" },
  ])
  await expectNoHorizontalScroll(page)
})

test.describe("a refused transition", () => {
  test.use({ allow: { responses: [{ url: `/v1${TRANSITIONS}`, status: 422 }] } })

  test("is shown as the API wrote it", async ({ page }) => {
    await open(page, "ops", `/ops/requests/${REQUEST}`, {
      [`GET ${REQUESTS}/${REQUEST}`]: one(pueRequest()),
      "GET /workspace/w1/farm/villages/v1/energy": energy,
      // Withdrawn by the farmer since this screen loaded.
      [`POST ${TRANSITIONS}`]: { status: 422, body: { message: "illegal transition withdrawn -> under_review" } },
    })

    await page.getByTestId("action-start_review").click()

    await expect(page.getByTestId("review-error")).toContainText("illegal transition withdrawn -> under_review")
    await expectNoHorizontalScroll(page)
  })
})

test.describe("approving with no note", () => {
  test.use({ allow: { responses: [{ url: `/v1${TRANSITIONS}`, status: 422 }] } })

  test("asks first, then shows the API's refusal at the note field", async ({ page }) => {
    const sent = await open(page, "ops", `/ops/requests/${REQUEST}`, {
      [`GET ${REQUESTS}/${REQUEST}`]: one(pueRequest({ status: "under_review" })),
      "GET /workspace/w1/farm/villages/v1/energy": energy,
      [`POST ${TRANSITIONS}`]: noteRequired,
    })

    await page.getByTestId("action-approve").click()
    await page.getByTestId("confirm-dialog-confirm").click()

    await expect(page.getByTestId("decision-note-error")).toContainText("The note field is required to approve or reject.")
    await expect(page.getByTestId("decision-note")).toHaveAttribute("aria-invalid", "true")
    await expect(page.getByTestId("review-error")).toHaveCount(0)
    expect(sent.filter((s) => s.key === `POST ${TRANSITIONS}`).map((s) => s.body)).toEqual([{ action: "approve" }])
    await expectNoHorizontalScroll(page)
  })
})
