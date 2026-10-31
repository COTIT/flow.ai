import type { Page } from "@playwright/test"

import { mockApi } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, villages } from "./api"
import {
  CYCLE, DOCUMENT, FARM, HARVEST, HOUSEHOLD, PERSON, PLOT, crops, documents, holdings, person, record, tourSeen,
} from "./officer-records.api"

const DOWNLOAD = "https://files.example/title-deed.pdf?sig=e2e"

async function open(page: Page, correction: { status: number; body: unknown }) {
  await seedSession(page)
  await tourSeen(page)
  await mockApi(page, {
    "GET /farm/me": me(),
    "GET /workspace/w1/farm/villages": villages,
    [`GET /workspace/w1/farm/persons/${PERSON}`]: { body: { data: person() } },
    [`GET /workspace/w1/farm/persons/${PERSON}/login-history`]: { body: { data: [] } },
    "GET /workspace/w1/farm/crops": crops,
    [`GET /workspace/w1/farm/plots/${PLOT}/documents`]: documents,
    [`GET /workspace/w1/farm/plots/${PLOT}/documents/${DOCUMENT}`]: {
      body: { data: { ...(documents.body as { data: object[] }).data[0], download_url: DOWNLOAD, download_expires_at: "2026-11-01T09:00:00Z" } },
    },
  })
  let label = "Riverside farm"
  await record(page, "GET", `/workspace/w1/farm/persons/${PERSON}/holdings`, () => ({ body: holdings(label) }))
  const sent: unknown[] = []
  await record(page, "PUT", `/workspace/w1/farm/farms/${FARM}`, () => {
    if (correction.status < 400) label = "Hillside farm"
    return correction
  })
  await page.route(`**/v1/workspace/w1/farm/farms/${FARM}`, async (route) => {
    if (route.request().method() === "PUT") sent.push(route.request().postDataJSON())
    await route.fallback()
  })
  await page.context().route("https://files.example/**", (route) =>
    // HTML, not a PDF: a PDF would download and abort the navigation the test waits for.
    route.fulfill({ status: 200, contentType: "text/html", body: "<title>document</title>" }))
  await page.goto(`/officer/people/${PERSON}`)
  return sent
}

test("an officer reads a person's household, farm, plot, cycle, figures and documents, and corrects the farm", async ({ page }) => {
  const sent = await open(page, { status: 200, body: { data: {} } })

  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Neema Mwakalinga")
  await expect(page.getByTestId(`household-${HOUSEHOLD}`)).toContainText("Neema Mwakalinga, Juma Mwakalinga")
  // Registered by this officer: someone else verifies it (the API enforces; this only says so).
  await expect(page.getByTestId(`verify-needs-second-staff-${HOUSEHOLD}`)).toBeVisible()
  await expect(page.getByTestId(`farm-${FARM}`)).toContainText("Riverside farm")
  await expect(page.getByTestId(`plot-${PLOT}`)).toContainText("Riverside")
  await expect(page.getByTestId(`cycle-${CYCLE}`)).toContainText("Maize · Masika")
  await expect(page.getByTestId(`harvest-${HARVEST}`)).toContainText("4,100.00 kg")
  await expect(page.getByTestId("plot-document")).toHaveText("title-deed.pdf")

  const popup = page.waitForEvent("popup")
  await page.getByTestId("plot-document").click()
  await (await popup).waitForURL(DOWNLOAD)

  await page.getByTestId(`edit-farm-${FARM}`).click()
  await page.getByTestId("edit-label").fill("Hillside farm")
  await page.getByTestId("officer-edit-save").click()

  await expect(page.getByTestId(`farm-${FARM}`)).toContainText("Hillside farm")
  expect(sent).toEqual([{ label: "Hillside farm", latitude: "-7.71", longitude: "35.71" }])
  await expectNoHorizontalScroll(page)
})

test.describe("a refused correction", () => {
  test.use({ allow: { responses: [{ url: `/v1/workspace/w1/farm/farms/${FARM}`, status: 422 }] } })

  test("keeps the dialog open with the API's message", async ({ page }) => {
    await open(page, { status: 422, body: { message: "invalid", errors: { latitude: ["latitude must be between -90 and 90"] } } })
    await page.getByTestId(`edit-farm-${FARM}`).click()
    await page.getByTestId("edit-label").fill("Hillside farm")
    await page.getByTestId("officer-edit-save").click()

    await expect(page.getByTestId("officer-edit-error")).toHaveText("latitude must be between -90 and 90")
    await expect(page.getByTestId(`farm-${FARM}`)).toContainText("Riverside farm")
    await expectNoHorizontalScroll(page)
  })
})
