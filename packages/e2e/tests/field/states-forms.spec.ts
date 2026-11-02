import type { Page } from "@playwright/test"

import { mockApi, type MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, role, villages } from "./api"
import { MILL, mill, one as oneMachine } from "./equipment.api"
import { eligibilityBefore, farmerMe } from "./farmer.api"
import { FARM, PERSON, PLOT, crops, documents, holdings, person, record, tourSeen } from "./officer-records.api"
import { REQUEST, estimatePreview, one, pueRequest, requestList } from "./requests.api"
import { factorRefused, ilundo, mlowa, villageList } from "./villages.api"

/**
 * States plan step 3 (gaps 4-5): a 422 lands at the field it names, and the banner does not repeat
 * it; a save that worked says so in a success Notice (role=status). One form per group.
 */
const success = (page: Page, testId: string) => page.getByTestId(testId).getByRole("status")

/** Officer: correcting a farm from the person screen. */
async function openPerson(page: Page, correction: { status: number; body: unknown }) {
  await seedSession(page)
  await tourSeen(page)
  await mockApi(page, {
    "GET /farm/me": me(),
    "GET /workspace/w1/farm/villages": villages,
    [`GET /workspace/w1/farm/persons/${PERSON}`]: { body: { data: person() } },
    [`GET /workspace/w1/farm/persons/${PERSON}/login-history`]: { body: { data: [] } },
    "GET /workspace/w1/farm/crops": crops,
    [`GET /workspace/w1/farm/plots/${PLOT}/documents`]: documents,
  })
  let label = "Riverside farm"
  await record(page, "GET", `/workspace/w1/farm/persons/${PERSON}/holdings`, () => ({ body: holdings(label) }))
  await record(page, "PUT", `/workspace/w1/farm/farms/${FARM}`, () => {
    if (correction.status < 400) label = "Hillside farm"
    return correction
  })
  await page.goto(`/officer/people/${PERSON}`)
  await page.getByTestId(`edit-farm-${FARM}`).click()
}

test.describe("officer: a refused correction", () => {
  test.use({ allow: { responses: [{ url: `/v1/workspace/w1/farm/farms/${FARM}`, status: 422 }] } })

  test("shows the API's text at the latitude field, not in the banner", async ({ page }) => {
    await openPerson(page, { status: 422, body: { message: "invalid", errors: { latitude: ["latitude must be between -90 and 90"] } } })
    // In range here; the API is what refuses it.
    await page.getByTestId("edit-latitude").fill("-7.72")
    await page.getByTestId("officer-edit-save").click()

    await expect(page.getByTestId("edit-latitude-error")).toContainText("latitude must be between -90 and 90")
    await expect(page.getByTestId("edit-latitude")).toBeFocused()
    await expect(page.getByTestId("officer-edit-error")).toHaveCount(0)
    await expectNoHorizontalScroll(page)
  })
})

test("officer: a saved correction says so", async ({ page }) => {
  await openPerson(page, { status: 200, body: { data: {} } })
  await page.getByTestId("edit-label").fill("Hillside farm")
  await page.getByTestId("officer-edit-save").click()

  await expect(page.getByTestId(`farm-${FARM}`)).toContainText("Hillside farm")
  await expect(success(page, "person-saved")).toContainText("Correction saved")
  await expectNoHorizontalScroll(page)
})

const REQUESTS = "/workspace/w1/farm/pue-requests"

/** Farmer: filing a request for a machine, and withdrawing it. */
async function openFarmer(page: Page, path: string, routes: Record<string, MockReply>) {
  await seedSession(page)
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["farmer:u1"])))
  const table: Record<string, MockReply> = {
    "GET /farm/me": farmerMe,
    "GET /workspace/w1/farm/surveys/eligibility": eligibilityBefore,
    "GET /workspace/w1/farm/villages": villages,
    [`POST ${REQUESTS}/estimate`]: estimatePreview({ quantity: 1, hours_per_day: "6", days_per_week: "5" }),
    ...routes,
  }
  await mockApi(page, table)
  await page.goto(path)
  return table
}

test.describe("farmer: a refused request", () => {
  test.use({ allow: { responses: [{ url: `/v1${REQUESTS}`, status: 422 }] } })

  test("shows the API's text at the quantity field, not in the banner", async ({ page }) => {
    await openFarmer(page, `/farm/equipment/${MILL}`, {
      [`GET /workspace/w1/farm/equipment/${MILL}`]: oneMachine(mill),
      [`POST ${REQUESTS}`]: { status: 422, body: { message: "invalid", errors: { quantity: ["at most 3 machines per request"] } } },
    })
    await expect(page.getByTestId("request-hours")).toHaveValue("6")
    await page.getByTestId("equipment-acquisition-buy").check()
    await page.getByTestId("request-submit").click()

    await expect(page.getByTestId("request-quantity-error")).toContainText("at most 3 machines per request")
    await expect(page.getByTestId("request-quantity")).toBeFocused()
    await expect(page.getByTestId("request-error")).toHaveCount(0)
    await expectNoHorizontalScroll(page)
  })
})

test("farmer: a withdrawn request says so", async ({ page }) => {
  const withdrawn = pueRequest({ status: "withdrawn" })
  const table = await openFarmer(page, `/farm/requests/${REQUEST}`, {
    [`GET ${REQUESTS}/${REQUEST}`]: one(pueRequest()),
    [`GET ${REQUESTS}`]: requestList(pueRequest()),
    [`POST ${REQUESTS}/${REQUEST}/transitions`]: one(withdrawn),
  })
  // Later reads see the withdrawn request.
  await page.route(`**/v1${REQUESTS}/${REQUEST}/transitions`, async (route) => {
    table[`GET ${REQUESTS}/${REQUEST}`] = one(withdrawn)
    await route.fallback()
  })
  await page.getByTestId("request-action-withdraw").click()

  await expect(success(page, "request-action-done")).toContainText("Request withdrawn")
  await expect(page.getByTestId("request-action-withdraw")).toHaveCount(0)
  await expectNoHorizontalScroll(page)
})

const VILLAGES = "/workspace/w1/farm/villages"

/** Ops: adding a village. */
async function openVillages(page: Page, save: MockReply) {
  await seedSession(page)
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["ops:u1"])))
  const routes: Record<string, MockReply> = {
    "GET /farm/me": me({ name: "Omari Ops", memberships: [role("ops", null)] }),
    [`GET ${VILLAGES}`]: villageList(ilundo),
    [`POST ${VILLAGES}`]: save,
  }
  await mockApi(page, routes)
  await page.route(`**/v1${VILLAGES}`, async (route) => {
    if (route.request().method() === "POST" && (save.status ?? 200) < 400) routes[`GET ${VILLAGES}`] = villageList(ilundo, mlowa)
    await route.fallback()
  })
  await page.goto("/ops/villages")
  await page.getByTestId("village-create-open").click()
  await page.getByTestId("village-name").fill("Mlowa")
  await page.getByTestId("village-code").fill("MLW")
  await page.getByTestId("village-capacity").fill("250")
  await page.getByTestId("village-simultaneity").fill("1.5")
  await page.getByTestId("village-create-submit").click()
}

test.describe("ops: a refused village", () => {
  test.use({ allow: { responses: [{ url: `/v1${VILLAGES}`, status: 422 }] } })

  test("shows the API's text at the simultaneity field, not in the banner; editing it clears it", async ({ page }) => {
    await openVillages(page, factorRefused)

    const fieldError = page.getByTestId("village-simultaneity-error")
    await expect(fieldError).toContainText("simultaneity factor must be more than 0 and at most 1")
    await expect(page.getByTestId("village-simultaneity")).toBeFocused()
    await expect(page.getByTestId("village-create-error")).toHaveCount(0)
    await expectNoHorizontalScroll(page)

    await page.getByTestId("village-simultaneity").fill("0.65")
    await expect(fieldError).toHaveCount(0)
  })
})

test("ops: an added village says so, until the next edit", async ({ page }) => {
  await openVillages(page, { status: 201, body: { data: mlowa } })

  await expect(page.getByTestId("village-row")).toHaveCount(2)
  await expect(success(page, "village-saved")).toContainText("Village added")
  await expectNoHorizontalScroll(page)

  await page.getByTestId("village-name").fill("Ruaha")
  await expect(page.getByTestId("village-saved")).toHaveCount(0)
})
