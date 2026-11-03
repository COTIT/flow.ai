import type { Page } from "@playwright/test"

import { mockApi } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, role, villages } from "./api"
import { CYCLE, PLOT as CYCLE_PLOT, before, crops as cycleCrops, cycle, page as cyclePage, plot as cyclePlotReply } from "./cycle.api"
import { FARM, crops, farm, page as listPage, plot, record, tourSeen } from "./officer-records.api"
import { crops as registerCrops } from "./register.api"
import { V, VILLAGE, W, crops as towerCrops, energy, matches, pipeline, production, quality, villages as towerVillages } from "./tower.api"

/**
 * States plan step 4 (gap 6): errors that used to be swallowed are shown. A refused verify on the
 * farm and cycle screens, a Tower read that failed (its retry re-reads THAT read), and a draft the
 * device could not store.
 */
const REFUSAL = "this record was changed since you opened it"

/** The first verify is refused; "Try again" sends it once more and it goes through. */
async function verifyRefusedOnce(page: Page, path: string) {
  let calls = 0
  const sent = await record(page, "POST", path, () =>
    ++calls === 1 ? { status: 422, body: { message: REFUSAL } } : { body: { data: {} } },
  )
  return sent
}

async function confirmVerify(page: Page, testId: string) {
  await page.getByTestId(testId).click()
  await page.getByTestId("confirm-dialog-confirm").click()
}

test.describe("officer: a refused verify", () => {
  test.use({
    allow: {
      responses: [
        { url: `/v1/workspace/w1/farm/farms/${FARM}/verification`, status: 422 },
        { url: `/v1/workspace/w1/farm/crop-cycles/${CYCLE}/verification`, status: 422 },
      ],
    },
  })

  test("on the farm screen shows the API's refusal, and Try again re-sends it", async ({ page }) => {
    await seedSession(page)
    await tourSeen(page)
    await mockApi(page, {
      "GET /farm/me": me(),
      "GET /workspace/w1/farm/villages": villages,
      [`GET /workspace/w1/farm/farms/${FARM}`]: { body: { data: farm() } },
      "GET /workspace/w1/farm/crops": crops,
    })
    await record(page, "GET", "/workspace/w1/farm/plots", () => ({ body: listPage([plot()]) }))
    const sent = await verifyRefusedOnce(page, `/workspace/w1/farm/farms/${FARM}/verification`)
    await page.goto(`/officer/farms/${FARM}`)

    await confirmVerify(page, `verify-farm-${FARM}`)
    const error = page.getByTestId("verify-error")
    await expect(error.getByRole("alert")).toContainText(REFUSAL)
    await expectNoHorizontalScroll(page)

    await error.getByTestId("error-retry").click()
    await expect(page.getByTestId("verify-done").getByRole("status")).toBeVisible()
    await expect(page.getByTestId("verify-error")).toHaveCount(0)
    expect(sent).toHaveLength(2)
  })

  test("on the cycle screen shows the API's refusal, and Try again re-sends it", async ({ page }) => {
    await seedSession(page)
    await tourSeen(page)
    await mockApi(page, {
      "GET /farm/me": me(),
      "GET /workspace/w1/farm/villages": villages,
      [`GET /workspace/w1/farm/crop-cycles/${CYCLE}`]: cycle,
      [`GET /workspace/w1/farm/plots/${CYCLE_PLOT}`]: cyclePlotReply,
      "GET /workspace/w1/farm/crops": cycleCrops,
      [`GET /workspace/w1/farm/crop-cycles/${CYCLE}/harvest-reports`]: { body: cyclePage(before) },
    })
    const sent = await verifyRefusedOnce(page, `/workspace/w1/farm/crop-cycles/${CYCLE}/verification`)
    await page.goto(`/officer/cycles/${CYCLE}`)

    await confirmVerify(page, `verify-crop_cycle-${CYCLE}`)
    const error = page.getByTestId("verify-error")
    await expect(error.getByRole("alert")).toContainText(REFUSAL)
    await expectNoHorizontalScroll(page)

    await error.getByTestId("error-retry").click()
    await expect(page.getByTestId("verify-done").getByRole("status")).toBeVisible()
    expect(sent).toHaveLength(2)
  })
})

test.describe("ops: a Tower read that fails", () => {
  test.use({ allow: { responses: [{ url: `/v1${V}/demand-matches`, status: 503 }] } })

  test("shows the error, and Try again re-reads that read only", async ({ page }) => {
    await seedSession(page)
    await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["ops:u1"])))
    await mockApi(page, {
      "GET /farm/me": me({ name: "Omari Ops", memberships: [role("ops", null)] }),
      [`GET ${W}/villages`]: towerVillages,
      [`GET ${W}/crops`]: towerCrops,
      [`GET ${V}/energy`]: energy,
      [`GET ${V}/pue-pipeline`]: pipeline,
      [`GET ${V}/data-quality`]: quality,
    })
    const productionReads = await record(page, "GET", `${V}/production`, () => ({ body: (production as { body: unknown }).body }))
    let down = true
    const marketReads = await record(page, "GET", `${V}/demand-matches`, () =>
      down ? { status: 503, body: { message: "Service unavailable" } } : { body: (matches as { body: unknown }).body },
    )
    await page.goto(`/ops/tower?village=${VILLAGE}`)

    await expect(page.getByTestId("error-state")).toBeVisible()
    await expect(page.getByTestId("error-state").getByRole("alert")).toBeVisible()
    await expectNoHorizontalScroll(page)
    expect(marketReads).toHaveLength(1)

    down = false
    await page.getByTestId("error-retry").click()
    await expect(page.getByTestId("tile-market")).toContainText("Iringa Grain Traders")
    expect(marketReads).toHaveLength(2)
    expect(productionReads).toHaveLength(1)
  })
})

test("officer: Register says so when the device cannot store the draft", async ({ page }) => {
  // Storage blocked (private mode, a full disk): opening IndexedDB throws.
  await page.addInitScript(() => {
    IDBFactory.prototype.open = () => {
      throw new DOMException("The operation is insecure.", "SecurityError")
    }
  })
  await seedSession(page)
  await tourSeen(page)
  await mockApi(page, {
    "GET /farm/me": me(),
    "GET /workspace/w1/farm/villages": villages,
    "GET /workspace/w1/farm/crops": registerCrops,
  })
  await page.goto("/officer/register")
  await expect(page).toHaveURL(/draft=/)

  await page.getByTestId("register-given-name").fill("Neema")
  const warning = page.getByTestId("draft-storage-error").getByRole("status")
  await expect(warning).toContainText("Could not save this draft on this device")
  await expectNoHorizontalScroll(page)
})
