import type { Page } from "@playwright/test"

import { mockApi, type MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, role, villages } from "./api"
import { categories, codeTaken, currencies, equipment, mill, milling, one } from "./equipment.api"

const CATS = "/workspace/w1/farm/equipment-categories"
const EQUIPMENT = "/workspace/w1/farm/equipment"
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/

type Sent = { key: string; body: unknown }

/**
 * Signed in as ops on the whole account. Each write is recorded; `effects` run before it is
 * answered so the reads that follow see the API's new state.
 */
async function open(
  page: Page,
  { extra = {}, effects = {} }: {
    extra?: Record<string, MockReply>
    effects?: Record<string, (routes: Record<string, MockReply>, body: unknown) => void>
  } = {},
) {
  await seedSession(page)
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["ops:u1"])))
  const routes: Record<string, MockReply> = {
    "GET /farm/me": me({ name: "Omari Ops", memberships: [role("ops", null)] }),
    "GET /workspace/w1/farm/villages": villages,
    "GET /workspace/w1/currencies": currencies,
    [`GET ${CATS}`]: categories(milling),
    [`GET ${EQUIPMENT}`]: equipment(mill),
    ...extra,
  }
  await mockApi(page, routes)
  const sent: Sent[] = []
  await page.route("**/v1/**", async (route) => {
    const request = route.request()
    const key = `${request.method()} ${new URL(request.url()).pathname.replace(/^\/v1/, "")}`
    const body = request.method() === "GET" ? undefined : request.postDataJSON()
    if (request.method() !== "GET") sent.push({ key, body })
    effects[key]?.(routes, body)
    await route.fallback()
  })
  await page.goto("/ops/catalogue")
  return sent
}

test("ops sees the catalogue: machines with indicative prices, and their categories", async ({ page }) => {
  await open(page)

  const row = page.getByTestId("catalogue-row")
  await expect(row).toHaveCount(1)
  await expect(row).toContainText("Maize mill")
  await expect(row.getByTestId("catalogue-price")).toContainText("TZS 22,000,000.00")
  await expect(row.getByTestId("catalogue-price")).toContainText(/indicative/i)
  await expect(page.getByTestId("category-row")).toContainText(["Milling"])
  await expectNoHorizontalScroll(page)
})

test("ops adds a category, then a machine in it, each with a device-made id", async ({ page }) => {
  const cold = { ...milling, id: "cat2", code: "COLD", name_en: "Cold storage", name_sw: "Hifadhi baridi" }
  const hammer = { ...mill, id: "eq2", code: "HAMMER", name_en: "Hammer mill", name_sw: "Mashine ya nyundo" }
  const sent = await open(page, {
    extra: {
      [`POST ${CATS}`]: { status: 201, body: { data: cold } },
      [`POST ${EQUIPMENT}`]: { status: 201, body: { data: hammer } },
    },
    effects: {
      [`POST ${CATS}`]: (routes) => { routes[`GET ${CATS}`] = categories(milling, cold) },
      [`POST ${EQUIPMENT}`]: (routes) => { routes[`GET ${EQUIPMENT}`] = equipment(mill, hammer) },
    },
  })

  await page.getByTestId("category-code").fill("COLD")
  await page.getByTestId("category-name-en").fill("Cold storage")
  await page.getByTestId("category-name-sw").fill("Hifadhi baridi")
  await page.getByTestId("category-submit").click()
  await expect(page.getByTestId("category-row")).toHaveCount(2)
  const category = sent.find((s) => s.key === `POST ${CATS}`)!.body
  expect(category).toEqual({ id: expect.stringMatching(UUID), code: "COLD", name_en: "Cold storage", name_sw: "Hifadhi baridi" })

  await page.getByTestId("catalogue-create-open").click()
  await page.getByTestId("catalogue-code").fill("HAMMER")
  await page.getByTestId("catalogue-name-en").fill("Hammer mill")
  await page.getByTestId("catalogue-name-sw").fill("Mashine ya nyundo")
  await page.getByTestId("catalogue-category").click()
  await page.getByRole("option", { name: "Cold storage" }).click()
  await page.getByTestId("catalogue-power").fill("5.5")
  await page.getByTestId("catalogue-can-rent").check()
  await page.getByTestId("catalogue-buy-price").fill("22000000")
  await page.getByTestId("catalogue-rent-price").fill("60000.50")
  await page.getByTestId("catalogue-create-submit").click()

  await expect(page.getByTestId("catalogue-row")).toHaveCount(2)
  const machine = sent.find((s) => s.key === `POST ${EQUIPMENT}`)!.body
  // Figures as decimal text, money as integer minor units of the account's currency.
  expect(machine).toEqual({
    id: expect.stringMatching(UUID),
    category_id: "cat2",
    code: "HAMMER",
    name_en: "Hammer mill",
    name_sw: "Mashine ya nyundo",
    rated_power_kw: "5.5",
    typical_hours_per_day: null,
    typical_days_per_week: null,
    can_rent: true,
    can_buy: true,
    indicative_price_minor: 2200000000,
    indicative_rent_per_day_minor: 6000050,
    currency_id: "c-tzs",
  })
  await expectNoHorizontalScroll(page)
})

test.describe("a refused category", () => {
  test.use({ allow: { responses: [{ url: `/v1${CATS}`, status: 422 }] } })

  test("shows the API's message as written, at the code", async ({ page }) => {
    await open(page, { extra: { [`POST ${CATS}`]: codeTaken } })

    await page.getByTestId("category-code").fill("MILLING")
    await page.getByTestId("category-name-en").fill("Milling again")
    await page.getByTestId("category-name-sw").fill("Kusaga tena")
    await page.getByTestId("category-submit").click()

    await expect(page.getByTestId("category-code-error")).toContainText("this code is already used in this workspace")
    await expect(page.getByTestId("category-create-error")).toHaveCount(0)
    await expectNoHorizontalScroll(page)
  })
})

test("a replayed create (200, the stored row) is accepted like a new one", async ({ page }) => {
  const sent = await open(page, { extra: { [`POST ${CATS}`]: one(milling) } })

  await page.getByTestId("category-code").fill("MILLING")
  await page.getByTestId("category-name-en").fill("Milling")
  await page.getByTestId("category-name-sw").fill("Kusaga")
  await page.getByTestId("category-submit").click()

  await expect(page.getByTestId("category-code")).toHaveValue("")
  await expect(page.getByTestId("category-create-error")).toHaveCount(0)
  expect(sent.filter((s) => s.key === `POST ${CATS}`)).toHaveLength(1)
})
