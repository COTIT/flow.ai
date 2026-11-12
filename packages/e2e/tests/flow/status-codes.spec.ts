import type { Page } from "@playwright/test"

import { markInjected } from "../../src/test"
import { expect, test } from "./storefront-api"
import { scene, stockRoutes } from "./task"
import { fakeAgent, startText, v } from "./voice"

// SIMULATED voice + API. Guard: a failed or late answer never puts its HTTP status on screen.

test.beforeEach(({ api }) => void api)

const STATUSES = [500, 502, 503] as const
/** The status itself as a word, or its reason phrase; data numbers like "500g" do not match. */
const shown = (status: number) =>
  new RegExp(
    `\\b${status}\\b|Internal Server Error|Bad Gateway|Service Unavailable`,
    "i"
  )

/** The next request to `path` answers `status` (an HTML gateway page for 502), after `lateMs`. */
async function failNext(page: Page, path: RegExp, status: number, lateMs = 0) {
  markInjected(page, path)
  let left = 1
  await page.route(path, async (route) => {
    if (left <= 0) return route.fallback()
    left--
    if (lateMs) await new Promise((r) => setTimeout(r, lateMs))
    await route.fulfill(
      status === 502
        ? { status, contentType: "text/html", body: "<html>Bad Gateway</html>" }
        : { status, json: { message: `Upstream failed with ${status}.` } }
    )
  })
  return { hit: () => left === 0 }
}

async function visibleText(page: Page) {
  return page.locator("body").innerText()
}

for (const status of STATUSES) {
  test(`${status} starting a conversation stays in plain words`, async ({
    page,
  }) => {
    await fakeAgent(page)
    await page.goto("/")
    const injected = await failNext(page, /\/api\/voice\/session/, status)
    await page.getByRole("button", { name: v.textInstead, exact: true }).click()
    await expect(page.getByRole("alert").first()).not.toBeEmpty()
    expect(injected.hit(), "the failure was injected").toBe(true)
    expect(await visibleText(page)).not.toMatch(shown(status))
  })

  test(`${status} on a tool call, answered late, stays in plain words`, async ({
    page,
  }) => {
    const agent = await fakeAgent(page)
    await page.goto("/")
    await startText(page)
    const injected = await failNext(page, /\/api\/voice\/tools/, status, 1500)
    agent.call("products_list", {})
    await expect(page.getByRole("alert").first()).not.toBeEmpty()
    expect(injected.hit(), "the failure was injected").toBe(true)
    expect(await visibleText(page)).not.toMatch(shown(status))
    // The agent hears the reason, never the status.
    await expect.poll(agent.results).toBe(1)
    const result = agent.sent.find((e) => e.type === "client_tool_result")
    expect(String(result?.result)).not.toMatch(shown(status))
  })

  test(`${status} on an approval stays in plain words`, async ({ page }) => {
    const agent = await fakeAgent(page)
    await stockRoutes(page)
    await page.goto("/")
    await startText(page)
    agent.call("stock_set_levels", {
      path: { product: "p-500" },
      body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 120 }] },
    })
    await expect(scene(page)).toContainText("Confirm action 135790")
    const injected = await failNext(page, /\/api\/voice\/confirm/, status, 1500)
    await scene(page).getByRole("button", { name: v.approve }).click()
    await expect(page.getByRole("alert").first()).not.toBeEmpty()
    expect(injected.hit(), "the failure was injected").toBe(true)
    expect(await visibleText(page)).not.toMatch(shown(status))
  })

  test(`${status} on a scene read stays in plain words`, async ({ page }) => {
    const injected = await failNext(page, /\/v1\/workspace\/w1\/orders/, status)
    await page.goto("/scenes/orders")
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
    await page.waitForTimeout(1500)
    expect(injected.hit(), "the failure was injected").toBe(true)
    expect(await visibleText(page)).not.toMatch(shown(status))
  })
}
