import type { Page } from "@playwright/test"

import { mockApi, type MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, role, villages } from "./api"
import { eligibilityBefore, farmerMe } from "./farmer.api"
import { categories, currencies, equipment, mill, milling } from "./equipment.api"
import { inputLoan, loans, rangeBackwards, womenLoan } from "./loans.api"

const LOANS = "/workspace/w1/farm/loan-products"
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/

test("a farmer sees the active loan listings with their indicative range, and asks at the office", async ({ page }) => {
  await seedSession(page)
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["farmer:u1"])))
  await mockApi(page, {
    "GET /farm/me": farmerMe,
    "GET /workspace/w1/farm/surveys/eligibility": eligibilityBefore,
    "GET /workspace/w1/farm/equipment": equipment(mill),
    [`GET ${LOANS}`]: loans(inputLoan),
  })
  const asked: string[] = []
  page.on("request", (r) => { if (r.url().includes(LOANS)) asked.push(new URL(r.url()).searchParams.get("active") ?? "") })
  await page.goto("/farm/equipment")

  await page.getByTestId("resources-tab-loan").click()
  const card = page.getByTestId("loan-item")
  await expect(card).toHaveCount(1)
  await expect(card).toContainText("Input loan")
  await expect(card).toContainText("Seed and fertiliser for a season.")
  await expect(card.getByTestId("loan-range")).toContainText(/TZS\s200,000\.00 – TZS\s1,500,000\.00/)
  await expect(card.getByTestId("loan-range")).toContainText(/indicative/i)
  await expect(card).toContainText("Ask at the Ruaha office")
  await expect(card.getByRole("link")).toHaveCount(0)
  expect(asked).toEqual(["true"])
  await expectNoHorizontalScroll(page)
})

/** Signed in as ops on the loan tab of the catalogue; each POST body is recorded. */
async function openOps(page: Page, post: MockReply) {
  await seedSession(page)
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["ops:u1"])))
  const routes: Record<string, MockReply> = {
    "GET /farm/me": me({ name: "Omari Ops", memberships: [role("ops", null)] }),
    "GET /workspace/w1/farm/villages": villages,
    "GET /workspace/w1/currencies": currencies,
    "GET /workspace/w1/farm/equipment-categories": categories(milling),
    "GET /workspace/w1/farm/equipment": equipment(mill),
    [`GET ${LOANS}`]: loans(inputLoan),
    [`POST ${LOANS}`]: post,
  }
  await mockApi(page, routes)
  const sent: unknown[] = []
  await page.route(`**/v1${LOANS}`, async (route) => {
    if (route.request().method() === "POST") {
      sent.push(route.request().postDataJSON())
      if ((post.status ?? 200) < 300) routes[`GET ${LOANS}`] = loans(inputLoan, womenLoan)
    }
    await route.fallback()
  })
  await page.goto("/ops/catalogue?kind=loan")
  await expect(page.getByTestId("catalogue-loan-row")).toHaveCount(1)
  await page.getByTestId("catalogue-create-open").click()
  await page.getByTestId("catalogue-code").fill("WOMEN")
  await page.getByTestId("catalogue-name-en").fill("Women group loan")
  await page.getByTestId("catalogue-name-sw").fill("Mkopo wa kikundi cha wanawake")
  await page.getByTestId("catalogue-min").fill("500000")
  await page.getByTestId("catalogue-max").fill("3000000.50")
  return sent
}

test("ops adds a loan listing: a device-made id and integer minor units, no finance terms", async ({ page }) => {
  const sent = await openOps(page, { status: 201, body: { data: womenLoan } })
  await expect(page.getByTestId("catalogue-create-panel")).not.toContainText(/interest|repay|deposit|instal/i)
  await page.getByTestId("catalogue-create-submit").click()

  await expect(page.getByTestId("catalogue-loan-row")).toHaveCount(2)
  await expect(page.getByTestId("catalogue-loan-row").nth(1)).toContainText(/TZS\s500,000\.00 –\s?TZS\s3,000,000\.50/)
  expect(sent).toEqual([{
    id: expect.stringMatching(UUID),
    code: "WOMEN",
    name_en: "Women group loan",
    name_sw: "Mkopo wa kikundi cha wanawake",
    description_en: null,
    description_sw: null,
    indicative_min_minor: 50000000,
    indicative_max_minor: 300000050,
    currency_id: "c-tzs",
  }])
  await expectNoHorizontalScroll(page)
})

test.describe("a refused loan listing", () => {
  test.use({ allow: { responses: [{ url: `/v1${LOANS}`, status: 422 }] } })

  test("shows the API's message as written, and a retry keeps the same id", async ({ page }) => {
    const sent = await openOps(page, rangeBackwards)
    await page.getByTestId("catalogue-create-submit").click()
    await expect(page.getByTestId("catalogue-create-error")).toContainText("the maximum must not be below the minimum")

    await page.getByTestId("catalogue-max").fill("3000001")
    await page.getByTestId("catalogue-create-submit").click()
    await expect.poll(() => sent.length).toBe(2)
    const [first, second] = sent as [{ id: string }, { id: string; indicative_max_minor: number }]
    expect(second.id).toBe(first.id)
    expect(second.indicative_max_minor).toBe(300000100)
    await expectNoHorizontalScroll(page)
  })
})
