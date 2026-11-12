// F2: My Routines over a SIMULATED API. Creating a routine stays a reviewed voice action.
import { expect, test } from "./storefront-api"
import { sc } from "./scenes-api"

// Every test signs in and gets the stand-in API, also those that never touch `api`.
test.beforeEach(({ api }) => void api)

const report = (id: string, active: boolean) => ({
  id,
  active,
  report: "sales_summary",
  frequency: "weekly",
  weekday: 1,
  day_of_month: null,
  hour: 7,
  email: "amina@genesy.test",
  time_zone: "Africa/Dar_es_Salaam",
  created_at: "2026-11-01T07:00:00Z",
  next_run_at: "2026-11-16T04:00:00Z",
  last_run_at: null,
  last_outcome: null,
  paused_reason: null,
})

test("pause, resume and delete a routine; empty state points to Genesy", async ({
  page,
  api,
}) => {
  let rows = [report("r1", true)]
  api
    .on("GET", "/workspace/w1/scheduled-reports", () => ({ data: rows }))
    .on("POST", "/workspace/w1/scheduled-reports/:id", ({ body }) => {
      rows = rows.map((r) => ({ ...r, ...(body as object) }))
      return { data: rows[0] }
    })
    .on("DELETE", "/workspace/w1/scheduled-reports/:id", () => {
      rows = []
      return { status: 204 }
    })
  await page.goto("/scenes/routines")
  const row = page.locator('[data-report="r1"]')
  await expect(row).toContainText("Sales report · Weekly")
  await row.getByRole("button", { name: sc.pause }).click()
  await expect(row).toContainText(sc.paused)
  await row.getByRole("button", { name: sc.resume }).click()
  await expect(row).toContainText(sc.active)
  await row.getByRole("button", { name: sc.delete }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: sc.delete })
    .click()
  await expect(page.getByText(sc.routines.emptyTitle)).toBeVisible()
  await expect(page.getByRole("button", { name: sc.routines.setUp })).toBeVisible()
  await expect(page.getByText(sc.planned)).toBeVisible()
})
