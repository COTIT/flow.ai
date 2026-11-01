import type { Page } from "@playwright/test"

import { mockApi, type MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { eligibilityBefore, farmerMe } from "./farmer.api"
import { MILL, equipment, mill, one } from "./equipment.api"
import { estimatePreview } from "./requests.api"

const EQUIPMENT = "/workspace/w1/farm/equipment"

async function open(page: Page, path: string, extra: Record<string, MockReply> = {}) {
  await seedSession(page)
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["farmer:u1"])))
  await mockApi(page, {
    "GET /farm/me": farmerMe,
    // The surveys nav badge.
    "GET /workspace/w1/farm/surveys/eligibility": eligibilityBefore,
    [`GET ${EQUIPMENT}`]: equipment(mill),
    [`GET ${EQUIPMENT}/${MILL}`]: one(mill),
    // The request form's live estimate.
    "POST /workspace/w1/farm/pue-requests/estimate": estimatePreview({ quantity: 1, hours_per_day: "6", days_per_week: "5" }),
    ...extra,
  })
  await page.goto(path)
}

test("a farmer lists equipment and opens one, at indicative prices", async ({ page }) => {
  await open(page, "/farm/equipment")

  const card = page.getByTestId("equipment-item")
  await expect(card).toHaveCount(1)
  await expect(card).toContainText("Maize mill")
  await expect(card).toContainText("15.000 kW")
  await expect(card.getByTestId("equipment-price")).toContainText("TZS 22,000,000.00")
  await expect(card.getByTestId("equipment-rent")).toContainText("TZS 50,000.00")
  await expect(page.getByTestId("resources-tab-loan")).toHaveCount(0)
  await expectNoHorizontalScroll(page)

  await card.getByRole("link").click()
  await expect(page).toHaveURL(new RegExp(`/farm/equipment/${MILL}$`))
  const detail = page.getByTestId("equipment-detail")
  await expect(detail.getByRole("heading", { level: 1 })).toHaveText("Maize mill")
  await expect(detail.getByTestId("equipment-price")).toContainText("TZS 22,000,000.00")
  // The request form (its own spec covers sending one).
  await expect(page.getByTestId("request-submit")).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test.describe("a machine from another account", () => {
  const OTHER = "51000000-0000-4000-8000-000000000099"
  test.use({ allow: { responses: [{ url: `/v1${EQUIPMENT}/${OTHER}`, status: 404 }] } })

  test("reads as not found", async ({ page }) => {
    await open(page, `/farm/equipment/${OTHER}`, {
      [`GET ${EQUIPMENT}/${OTHER}`]: { status: 404, body: { message: "Not found." } },
    })

    await expect(page.getByTestId("empty-state")).toContainText("Equipment not found")
    await expectNoHorizontalScroll(page)
  })
})
