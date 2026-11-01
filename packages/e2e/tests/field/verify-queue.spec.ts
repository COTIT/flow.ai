import type { Page } from "@playwright/test"

import { mockApi } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, villages } from "./api"
import { HARVEST, HOUSEHOLD, PERSON, cycle, harvest, household, page as listPage, person, provenance, record, tourSeen } from "./officer-records.api"

const OTHER_HOUSEHOLD = "81000000-0000-4000-8000-000000000002"
const FOUR_EYES = "a household must be verified by someone other than the officer who registered it"

/** The queue's five lists and the workspace-wide harvest figures; the person drops out once verified. */
async function open(page: Page, locale: "en" | "sw" = "en", path = "/officer/verify") {
  await seedSession(page)
  await tourSeen(page)
  await mockApi(page, {
    "GET /farm/me": me({ locale }),
    "GET /workspace/w1/farm/villages": villages,
    "GET /workspace/w1/farm/farms": { body: listPage([]) },
    "GET /workspace/w1/farm/plots": { body: listPage([]) },
    "GET /workspace/w1/farm/crop-cycles": { body: listPage([cycle]) },
    "GET /workspace/w1/farm/harvest-reports": { body: listPage([harvest]) },
    [`POST /workspace/w1/farm/households/${OTHER_HOUSEHOLD}/verification`]: { status: 422, body: { message: FOUR_EYES } },
  })
  let verified = false
  const reads = await record(page, "GET", "/workspace/w1/farm/persons", () => ({ body: listPage(verified ? [] : [person()]) }))
  await record(page, "GET", "/workspace/w1/farm/households", () => ({
    // One this officer registered (u1), and one whose registrar the app does not know: the API decides.
    body: listPage([household, { ...household, id: OTHER_HOUSEHOLD, label: "Mbwambo household", provenance: provenance() }]),
  }))
  await record(page, "POST", `/workspace/w1/farm/persons/${PERSON}/verification`, () => {
    verified = true
    return { body: { data: person({ provenance: provenance("verified") }) } }
  })
  await page.goto(path)
  return reads
}

async function verify(page: Page, testId: string) {
  await page.getByTestId(testId).click()
  await page.getByTestId("confirm-dialog-confirm").click()
}

test("the queue lists outstanding records from the API and a verified one leaves it", async ({ page }) => {
  const reads = await open(page)

  await expect(page.getByTestId("verify-queue-row")).toHaveCount(5)
  expect(reads[0]?.searchParams.get("verification")).toBe("unverified,pending")
  await expect(page.getByTestId(`verify-needs-second-staff-${HOUSEHOLD}`)).toBeVisible()
  // A cycle is named by its embedded crop; a harvest figure is in the queue and opens its cycle.
  await expect(page.getByTestId("verify-queue-row").filter({ hasText: "Maize · Masika" })).toHaveCount(1)
  const figure = page.getByTestId("verify-queue-row").filter({ has: page.getByTestId(`verify-harvest_report-${HARVEST}`) })
  await expect(figure).toContainText("Expected · 4100.00 kg")

  await verify(page, `verify-person-${PERSON}`)
  await expect(page.getByTestId("verify-queue-row")).toHaveCount(4)
  await expect(page.getByTestId(`verify-person-${PERSON}`)).toHaveCount(0)
  await expectNoHorizontalScroll(page)
})

test.describe("the four-eyes refusal", () => {
  test.use({
    allow: { responses: [{ url: `/v1/workspace/w1/farm/households/${OTHER_HOUSEHOLD}/verification`, status: 422 }] },
  })

  test("is shown as the API wrote it", async ({ page }) => {
    await open(page)
    await verify(page, `verify-household-${OTHER_HOUSEHOLD}`)
    // The app's English wording of the API's message (capitalised, full stop).
    await expect(page.getByTestId("error-state")).toContainText(/a household must be verified by someone other than the officer who registered it/i)
    await expect(page.getByTestId("verify-queue-row")).toHaveCount(5)
    await expectNoHorizontalScroll(page)
  })

  test("is shown in Swahili to a Swahili officer", async ({ page }) => {
    await open(page, "sw")
    await verify(page, `verify-household-${OTHER_HOUSEHOLD}`)
    await expect(page.getByTestId("error-state")).toContainText("Kaya lazima ihakikiwe na mtu mwingine, si afisa aliyeisajili.")
    await expectNoHorizontalScroll(page)
  })
})

test("the officer home counts what the queue lists, from the API's totals", async ({ page }) => {
  await open(page, "en", "/officer")
  // 1 person + 2 households + 1 cycle + 1 harvest figure outstanding: the totals the API sends, summed.
  await expect(page.getByTestId("officer-village")).toContainText("Ilundo")
  await expect(page.getByTestId("officer-unverified")).toContainText("5")
  await page.getByTestId("officer-unverified-link").click()
  await expect(page.getByTestId("verify-queue-row")).toHaveCount(5)
  await expectNoHorizontalScroll(page)
})
