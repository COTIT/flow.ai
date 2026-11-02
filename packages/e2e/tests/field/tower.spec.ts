import type { Page } from "@playwright/test"

import { mockApi, type MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, role } from "./api"
import {
  V, VILLAGE, W, crops, cycles, energy, farms, harvests, matches, persons, pipeline, plots, production, pueRequests,
  quality, refused, total, villages,
} from "./tower.api"

/** Signed in as ops (whole workspace), the tour seen, the village list mocked. */
async function open(page: Page, path: string, routes: Record<string, MockReply>) {
  await seedSession(page)
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["ops:u1"])))
  await mockApi(page, {
    "GET /farm/me": me({ name: "Omari Ops", memberships: [role("ops", null)] }),
    [`GET ${W}/villages`]: villages,
    [`GET ${W}/crops`]: crops,
    ...routes,
  })
  await page.goto(path)
}

const overview = {
  [`GET ${V}/production`]: production,
  [`GET ${V}/energy`]: energy,
  [`GET ${V}/pue-pipeline`]: pipeline,
  [`GET ${V}/demand-matches`]: matches,
  [`GET ${V}/data-quality`]: quality,
}

test("the ops home shows the API's queue totals", async ({ page }) => {
  await open(page, "/ops", {
    [`GET ${W}/pue-requests`]: total(2),
    [`GET ${W}/buyer-demands`]: total(1),
  })

  await expect(page.getByTestId("ops-queue-requests")).toContainText("2")
  await expect(page.getByTestId("ops-queue-demands")).toContainText("1")
  await expect(page.getByTestId("ops-queue-verification")).toHaveCount(0)
  await expectNoHorizontalScroll(page)
})

test("the Tower overview shows each village figure as the API gave it", async ({ page }) => {
  await open(page, `/ops/tower?village=${VILLAGE}`, overview)

  const productionTile = page.getByTestId("tile-production")
  await expect(productionTile).toContainText("Maize")
  await expect(productionTile).toContainText("3,200.00 kg")
  await expect(productionTile).toContainText("1.6000 ha")
  await expect(productionTile).toContainText(/not land area/i)

  await expect(page.getByTestId("tower-capacity")).toContainText("500.000 kW")
  await expect(page.getByTestId("tower-prospective-peak")).toHaveText("9.000 kW")
  await expect(page.getByTestId("tower-approved-peak")).toHaveText("10.800 kW")
  await expect(page.getByTestId("tower-headroom")).toContainText("489.200 kW")

  await expect(page.getByTestId("tile-market")).toContainText("Iringa Grain Traders")
  await expect(page.getByTestId("tile-pue")).toContainText("TZS 38,600,000.00")
  await expect(page.getByTestId("tile-pue-rent")).toContainText("TZS 150,000.00")

  const qualityTile = page.getByTestId("tile-quality")
  await expect(qualityTile).toContainText("3 / 6")
  await expect(qualityTile).toContainText("3 / 4")
  await expect(qualityTile).toContainText("5 / 7")
  await expectNoHorizontalScroll(page)
})

test("the production drill shows the cycle area as a sum of cycle areas, and every cycle", async ({ page }) => {
  await open(page, `/ops/tower/production?village=${VILLAGE}`, {
    [`GET ${V}/production`]: production,
    [`GET ${W}/crop-cycles`]: cycles,
    [`GET ${W}/harvest-reports`]: harvests,
    [`GET ${W}/plots`]: plots,
    [`GET ${W}/farms`]: farms,
    [`GET ${W}/persons`]: persons,
  })

  const group = page.getByTestId("production-group")
  await expect(group).toContainText("Planted area across cycles")
  await expect(group).toContainText("1.6000 ha")
  await expect(page.getByText(/The sum of cycle areas/)).toBeVisible()

  const row = page.getByTestId("production-row")
  await expect(row).toHaveCount(1)
  await expect(row).toContainText("Maize")
  await expect(row).toContainText("Neema Mushi")
  await expect(row).toContainText("Kipande A")
  await expect(row).toContainText("3,200.00 kg")
  await expectNoHorizontalScroll(page)
})

test("the data-quality drill states the API's ratio beside its records", async ({ page }) => {
  await open(page, `/ops/tower/quality?village=${VILLAGE}&metric=persons`, {
    [`GET ${V}/data-quality`]: quality,
    [`GET ${W}/persons`]: persons,
  })

  await expect(page.getByTestId("quality-ratio")).toContainText("3 / 6")
  await expect(page.getByTestId("quality-row")).toHaveCount(2)
  await expect(page.getByTestId("quality-table")).toContainText("Neema Mushi")
  await expectNoHorizontalScroll(page)
})

test("the energy drill keeps prospective and approved apart, with the API's peaks", async ({ page }) => {
  await open(page, `/ops/tower/energy?village=${VILLAGE}`, {
    [`GET ${V}/energy`]: energy,
    [`GET ${W}/pue-requests`]: pueRequests,
  })

  await expect(page.getByTestId("energy-group-prospective")).toContainText("Neema Mushi")
  await expect(page.getByTestId("energy-group-prospective")).toContainText("15.000 kW")
  await expect(page.getByTestId("energy-prospective-peak")).toHaveText("9.000 kW")
  await expect(page.getByTestId("energy-approved-peak")).toHaveText("10.800 kW")
  await expectNoHorizontalScroll(page)
})

test("the market drill lists the village's demand matches", async ({ page }) => {
  await open(page, `/ops/tower/market?village=${VILLAGE}`, { [`GET ${V}/demand-matches`]: matches })

  const row = page.getByTestId("market-row")
  await expect(row).toContainText("Iringa Grain Traders")
  await expect(row).toContainText("9,000.00 kg")
  await expect(row).toContainText("5,600.00 kg")
  await expect(row).toContainText("62.2%")
  await expect(page.getByTestId("market-opportunity")).toContainText("Shared")
  await expectNoHorizontalScroll(page)
})

test.describe("a refused read", () => {
  test.use({
    allow: {
      responses: [
        { url: `/v1${V}/data-quality`, status: 404 },
        { url: `/v1${V}/production`, status: 403 },
      ],
    },
  })

  test("a 404 is shown as the API wrote it", async ({ page }) => {
    await open(page, `/ops/tower/quality?village=${VILLAGE}&metric=persons`, {
      [`GET ${V}/data-quality`]: refused(404, "The requested resource could not be found."),
      [`GET ${W}/persons`]: persons,
    })
    await expect(page.getByTestId("error-state")).toContainText("The requested resource could not be found.")
    await expectNoHorizontalScroll(page)
  })

  test("a 403 is shown as the API wrote it", async ({ page }) => {
    await open(page, `/ops/tower/production?village=${VILLAGE}`, {
      [`GET ${V}/production`]: refused(403, "This action is unauthorized."),
      [`GET ${W}/crop-cycles`]: cycles,
      [`GET ${W}/harvest-reports`]: harvests,
      [`GET ${W}/plots`]: plots,
      [`GET ${W}/farms`]: farms,
      [`GET ${W}/persons`]: persons,
    })
    await expect(page.getByTestId("error-state")).toContainText("This action is unauthorized.")
    await expectNoHorizontalScroll(page)
  })
})
