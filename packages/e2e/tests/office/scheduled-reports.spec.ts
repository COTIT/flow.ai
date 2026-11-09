import { test as open } from "../../src/test"
import { expect, expectFits, fakeApi, test, type FakeApi } from "./fake-api"

// 64 hex: the shape of a real unsubscribe token.
const TOKEN = "ab".repeat(32)

const schedule = (over: Record<string, unknown> = {}) => ({
  id: "sr-1",
  report: "sales_summary",
  frequency: "weekly",
  weekday: 1,
  day_of_month: null,
  hour: 7,
  time_zone: "Africa/Nairobi",
  active: true,
  paused_reason: null as string | null,
  next_run_at: "2026-11-16T04:00:00Z",
  email: "amina@example.com",
  last_run_at: null,
  last_outcome: null,
  created_at: "2026-11-09T08:00:00Z",
  ...over,
})

function schedulesApi(api: FakeApi, rows = [schedule()]) {
  api
    .on("GET", "/workspace/:ws/scheduled-reports", () => ({ data: rows }))
    .on("POST", "/workspace/:ws/scheduled-reports/:id", ({ params, body }) => {
      const row = rows.find((r) => r.id === params[1])!
      const active = (body as { active: boolean }).active
      Object.assign(row, { active, paused_reason: active ? null : "user" })
      return { data: row }
    })
    .on("DELETE", "/workspace/:ws/scheduled-reports/:id", ({ params }) => {
      rows.splice(
        rows.findIndex((r) => r.id === params[1]),
        1
      )
      return { message: "ok" }
    })
}

test("lists my reports in words, pauses, resumes and deletes", async ({
  page,
  api,
}) => {
  schedulesApi(api, [
    schedule(),
    schedule({
      id: "sr-2",
      report: "stock",
      frequency: "daily",
      weekday: null,
      hour: 6,
      active: false,
      paused_reason: "access_lost",
    }),
  ])
  await page.goto("/settings/scheduled-reports")

  await expect(
    page.getByRole("heading", { name: "Scheduled reports" })
  ).toBeVisible()
  await expect(page.getByText("Every Monday around 07:00")).toBeVisible()
  await expect(page.getByText("Times in Africa/Nairobi")).toBeVisible()
  await expect(page.getByText("Low stock")).toBeVisible()
  await expectFits(page)
  if ((page.viewportSize()?.width ?? 0) >= 768)
    await expect(page.getByText("Paused: no access")).toBeVisible()

  await page.getByRole("button", { name: "Pause: Sales summary" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "Report paused." })
  ).toBeVisible()
  expect(
    api.callsTo("POST", "/workspace/:ws/scheduled-reports/:id")[0]?.body
  ).toEqual({ active: false })
  await page.getByRole("button", { name: "Resume: Sales summary" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "Report resumed." })
  ).toBeVisible()

  await page.getByRole("button", { name: "Delete: Low stock" }).click()
  await page.getByRole("button", { name: "Cancel" }).click()
  expect(
    api.callsTo("DELETE", "/workspace/:ws/scheduled-reports/:id")
  ).toHaveLength(0)
  await page.getByRole("button", { name: "Delete: Low stock" }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Delete" })
    .click()
  await expect(
    page.getByRole("status").filter({ hasText: "Report deleted." })
  ).toBeVisible()
  await expect(page.getByText("Low stock")).toHaveCount(0)
})

test("empty: says how to make one by voice", async ({ page, api }) => {
  schedulesApi(api, [])
  await page.goto("/settings/scheduled-reports")
  await expect(page.getByText("No scheduled reports yet.")).toBeVisible()
  await expect(
    page.getByText(
      "Ask the voice assistant, e.g. “Every Monday send me a sales summary”."
    )
  ).toBeVisible()
  await expectFits(page)
})

test.describe(() => {
  test.use({
    allow: {
      responses: [{ url: "/v1/workspace/w1/scheduled-reports", status: 500 }],
    },
  })

  test("load error: plain words and a retry", async ({ page, api }) => {
    let fail = true
    api.on("GET", "/workspace/:ws/scheduled-reports", () =>
      fail
        ? { status: 500, body: { message: "Server Error" } }
        : { data: [schedule()] }
    )
    await page.goto("/settings/scheduled-reports")
    const retry = page.getByRole("button", { name: "Try again" })
    await expect(retry).toBeVisible({ timeout: 20_000 })
    fail = false
    await retry.click()
    await expect(page.getByText("Every Monday around 07:00")).toBeVisible()
  })
})

open("unsubscribe: nothing stops until the click", async ({ page }) => {
  const api = await fakeApi(page)
  api.on("POST", "/scheduled-reports/unsubscribe", () => ({
    data: { report: "sales_summary" },
  }))
  await page.goto(`/unsubscribe?token=${TOKEN}`)

  await expect(
    page.getByRole("heading", { name: "Stop this report?" })
  ).toBeVisible()
  expect(api.callsTo("POST", "/scheduled-reports/unsubscribe")).toHaveLength(0)
  await expectFits(page)
  await page.getByRole("button", { name: "Stop the email" }).click()

  await expect(
    page.getByText("Done: you will not get Sales summary emails any more.")
  ).toBeVisible()
  expect(
    api.callsTo("POST", "/scheduled-reports/unsubscribe")[0]?.body
  ).toEqual({ token: TOKEN })
  await expect(
    page.getByRole("link", { name: "Manage scheduled reports" })
  ).toBeVisible()
})

open.describe(() => {
  open.use({
    allow: {
      responses: [{ url: "/v1/scheduled-reports/unsubscribe", status: 404 }],
    },
  })

  open(
    "unsubscribe: an old link says so; no token is incomplete",
    async ({ page }) => {
      const api = await fakeApi(page)
      api.on("POST", "/scheduled-reports/unsubscribe", () => ({
        status: 404,
        body: { message: "Not found." },
      }))
      await page.goto(`/unsubscribe?token=${TOKEN}`)
      await page.getByRole("button", { name: "Stop the email" }).click()
      await expect(
        page.getByText("This link is not valid any more")
      ).toBeVisible()

      await page.goto("/unsubscribe")
      await expect(page.getByText("This link is incomplete")).toBeVisible()
    }
  )
})
