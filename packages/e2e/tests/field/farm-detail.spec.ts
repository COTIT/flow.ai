import { mockApi } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, villages } from "./api"
import { FARM, PLOT, crops, farm, page as listPage, plot, record, tourSeen } from "./officer-records.api"

test("an officer reads a farm with its plots and corrects a plot", async ({ page }) => {
  await seedSession(page)
  await tourSeen(page)
  await mockApi(page, {
    "GET /farm/me": me(),
    "GET /workspace/w1/farm/villages": villages,
    [`GET /workspace/w1/farm/farms/${FARM}`]: { body: { data: farm() } },
    "GET /workspace/w1/farm/crops": crops,
  })
  let label = "Riverside"
  const plotReads = await record(page, "GET", "/workspace/w1/farm/plots", () => ({ body: listPage([plot({ label })]) }))
  const sent: unknown[] = []
  await page.route(`**/v1/workspace/w1/farm/plots/${PLOT}`, async (route) => {
    if (route.request().method() !== "PUT") return route.fallback()
    sent.push(route.request().postDataJSON())
    label = "Riverside upper"
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ data: plot({ label }) }) })
  })
  await page.goto(`/officer/farms/${FARM}`)

  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Riverside farm")
  await expect(page.getByTestId("farm-detail")).toContainText("-7.71, 35.71")
  await expect(page.getByTestId("farm-plot")).toHaveCount(1)
  await expect(page.getByTestId("farm-plot")).toContainText("Riverside")
  expect(plotReads[0]?.searchParams.get("farm_id")).toBe(FARM)

  await page.getByTestId(`edit-plot-${PLOT}`).click()
  await page.getByTestId("edit-label").fill("Riverside upper")
  await page.getByTestId("officer-edit-save").click()

  await expect(page.getByTestId("farm-plot")).toContainText("Riverside upper")
  expect(sent).toEqual([{ label: "Riverside upper", area_ha: "1.8", latitude: null, longitude: null }])
  await expectNoHorizontalScroll(page)
})

test.describe("a farm outside the officer's villages", () => {
  test.use({ allow: { responses: [{ url: `/v1/workspace/w1/farm/farms/${FARM}`, status: 404 }] } })

  test("is not found, not an error", async ({ page }) => {
    await seedSession(page)
    await tourSeen(page)
    await mockApi(page, {
      "GET /farm/me": me(),
      "GET /workspace/w1/farm/villages": villages,
      [`GET /workspace/w1/farm/farms/${FARM}`]: { status: 404, body: { message: "The requested resource could not be found." } },
      "GET /workspace/w1/farm/crops": crops,
    })
    await page.goto(`/officer/farms/${FARM}`)
    await expect(page.getByTestId("empty-state")).toBeVisible()
    await expect(page.getByTestId("farm-detail")).toHaveCount(0)
    await expectNoHorizontalScroll(page)
  })
})
