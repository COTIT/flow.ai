import type { Page } from "@playwright/test"

import { seedSession } from "../../src/session"
import { expect, markInjected, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { readBack, scene, stockRoutes } from "./task"
import { fakeAgent, startText, v } from "./voice"

// SIMULATED voice + API. P6-5 / P6-1d (flaky-network persona): approving in a tunnel.

/** Raw codes a person must never read (HTTP statuses, error names). */
const RAW =
  /\b(?:HTTP|status|error|code)[\s:]*[45]\d\d\b|\b(?:ERR_[A-Z_]+|TypeError|SyntaxError|Failed to fetch|NetworkError|Load failed|undefined|NaN)\b|is not valid JSON|Unexpected token/i

/** setOffline alone lets routed (mocked) requests through, so they are aborted too. */
async function tunnel(page: Page) {
  let down = false
  await page.route("**/*", (route) =>
    down ? route.abort("internetdisconnected") : route.fallback()
  )
  return async (on: boolean) => {
    if (on) markInjected(page, /./)
    down = on
    await page.context().setOffline(on)
  }
}

async function openReview(page: Page) {
  const agent = await fakeAgent(page)
  const confirms = await stockRoutes(page)
  await page.goto("/")
  await startText(page)
  agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 120 }] },
  })
  await expect(scene(page)).toContainText("Confirm action 135790")
  return confirms
}

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi(readBack("120.00")))
})

test("P6-1d: approving in a tunnel says so in plain words", async ({
  page,
}) => {
  await openReview(page)
  await (
    await tunnel(page)
  )(true)
  await scene(page).getByRole("button", { name: v.approve }).click()
  await expect(page.getByRole("alert").first()).toContainText(v.offlineNotSent)
  const alerts = (await page.getByRole("alert").allInnerTexts()).join(" | ")
  expect(alerts).not.toMatch(RAW)
})

test("P6-5: an approval made offline is never sent and stays approvable", async ({
  page,
}) => {
  const confirms = await openReview(page)
  const line = await tunnel(page)
  await line(true)
  await scene(page).getByRole("button", { name: v.approve }).click()
  await expect(page.getByRole("alert").first()).toContainText(v.offlineNotSent)
  expect(confirms, "offline: nothing reached Office").toHaveLength(0)
  // Not "may already have happened": the same review stays open, Approve included.
  await expect(scene(page)).toContainText("Confirm action 135790")
  await expect(
    scene(page).getByRole("button", { name: v.approve })
  ).toBeVisible()
  await line(false)
  await scene(page).getByRole("button", { name: v.approve }).click()
  await expect.poll(() => confirms.length).toBe(1)
})

test("P6-5: an approval whose answer was lost is checked again once the line is back, never re-sent", async ({
  page,
}) => {
  const confirms = await openReview(page)
  // The approval reaches Office, then the line drops before the answer comes back.
  markInjected(page, /\/api\/voice\/confirm/)
  await page.route("**/api/voice/confirm", (route) => {
    confirms.push({})
    return route.abort("connectionreset")
  })
  const checks: unknown[] = []
  await page.route("**/api/voice/tools", (route) => {
    const b = route.request().postDataJSON() as { command?: string }
    if (b.command !== "status") return route.fallback()
    checks.push(b)
    return route.fulfill({
      json: {
        status: "completed",
        actionId: "act-120",
        data: { message: "ok" },
      },
    })
  })
  await scene(page).getByRole("button", { name: v.approve }).click()
  await expect(page.getByRole("alert").first()).toContainText(
    v.requestErrors.unverified
  )
  expect(checks).toHaveLength(0)
  await page.evaluate(() => window.dispatchEvent(new Event("online")))
  await expect.poll(() => checks.length).toBe(1)
  expect(checks[0]).toMatchObject({
    command: "status",
    actionId: expect.any(String),
  })
  // The verified answer replaces "could not be verified".
  await expect(page.getByRole("alert").first()).toBeEmpty()
  // A second "online" has nothing left to check; the approval was sent once.
  await page.evaluate(() => window.dispatchEvent(new Event("online")))
  await page.waitForTimeout(500)
  expect(checks).toHaveLength(1)
  expect(confirms).toHaveLength(1)
})
