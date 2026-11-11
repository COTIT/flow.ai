// F2: full-screen scenes over a SIMULATED API. Checklist C H U V W X Y; design tests 2, 3, 9.
import type { Page } from "@playwright/test"

import { expect, test } from "./storefront-api"
import { HONEY, sc } from "./scenes-api"
import { orbAt, phase, s as shell } from "./shell"
import { fakeAgent, startText, watch } from "./voice"

// Every test signs in and gets the stand-in API, also those that never touch `api`.
test.beforeEach(({ api }) => void api)

const yourFlow = (page: Page) => page.getByRole("region", { name: sc.yourFlow })
const cards = (page: Page) => page.locator("[data-component]")

test("Stock Watch lists products below minimum; selecting one marks it for the conversation", async ({
  page,
}) => {
  await page.goto("/scenes/stock")
  await expect(
    page.getByRole("heading", { level: 1, name: "Stock Watch" })
  ).toBeVisible()
  const honey = page.locator(`[data-record="${HONEY}"]`)
  await expect(honey).toContainText("Raw Honey 500g")
  await expect(honey).toContainText("Available 14")
  await expect(honey).toContainText("Minimum 20")
  await expect(honey).toContainText("Short by 6")
  await honey.click()
  await expect(honey).toHaveAttribute("aria-pressed", "true")
})

test("Morning Pulse: pending orders first is saved and restored on the next visit, with no voice call", async ({
  page,
  api,
}) => {
  const sessions = watch(page, "session")
  let sockets = 0
  page.on("websocket", () => sockets++)
  await page.goto("/scenes/pulse")
  await expect(cards(page).first()).toHaveAttribute(
    "data-component",
    "sales-today"
  )
  await page.getByRole("button", { name: sc.pendingFirst }).click()
  await expect(
    page.getByRole("status").filter({ hasText: sc.pendingFirstDone })
  ).toBeVisible()
  await expect(cards(page).first()).toHaveAttribute(
    "data-component",
    "orders-pending"
  )
  const put = api.callsTo("PUT", "/workspace/w1/flow/scenes/pulse")
  expect(put).toHaveLength(1)
  // Next session: a fresh page load restores the layout from the API, data refetched, no LLM.
  await page.reload()
  await expect(cards(page).first()).toHaveAttribute(
    "data-component",
    "orders-pending"
  )
  await expect(cards(page).nth(1)).toHaveAttribute(
    "data-component",
    "sales-today"
  )
  expect(sessions.filter((s) => s.method === "POST")).toHaveLength(0)
  expect(sockets).toBe(0)
})

test("Sales Perspective: keep a period's view in Your Flow once, then remove and restore it", async ({
  page,
}) => {
  await page.goto("/scenes/sales")
  await page.getByLabel(sc.periodLabel).selectOption("this_month")
  await page.getByRole("button", { name: sc.keep }).click()
  await expect(
    page.getByRole("status").filter({ hasText: sc.saved })
  ).toBeVisible()
  await expect(page.getByRole("button", { name: sc.kept })).toBeDisabled()
  await page.goto("/")
  const kept = yourFlow(page).getByRole("link", { name: /Sales · This month/ })
  await expect(kept).toHaveCount(1)
  await kept.click()
  await page.getByRole("button", { name: sc.remove }).click()
  await expect(page).toHaveURL(/\/$/)
  await expect(
    yourFlow(page).getByRole("link", { name: /Sales · This month/ })
  ).toHaveCount(0)
  // A built-in removed goes to Hidden scenes and comes back with Restore.
  await page.goto("/scenes/routines")
  await page.getByRole("button", { name: sc.remove }).click()
  await expect(
    yourFlow(page).getByRole("link", { name: /My Routines/ })
  ).toHaveCount(0)
  await page.getByText(sc.hidden).click()
  await page.getByRole("button", { name: `${sc.restore} My Routines` }).click()
  await expect(
    yourFlow(page).getByRole("link", { name: /My Routines/ })
  ).toBeVisible()
})

test("Refresh refetches the scene's data without any voice call", async ({
  page,
  api,
}) => {
  const sessions = watch(page, "session")
  await page.goto("/scenes/orders")
  await expect(page.getByText("ORD-0042")).toBeVisible()
  await expect(page.locator("[data-freshness]")).toHaveAttribute(
    "data-freshness",
    "fresh"
  )
  const before = api.callsTo("GET", "/workspace/w1/orders").length
  await page.getByRole("button", { name: sc.refresh }).click()
  await expect
    .poll(() => api.callsTo("GET", "/workspace/w1/orders").length)
    .toBeGreaterThan(before)
  expect(sessions).toHaveLength(0)
})

test("an unknown scene says so inside the frame", async ({ page }) => {
  await page.goto("/scenes/nope")
  await expect(page.getByText(sc.notFound)).toBeVisible()
})

test("a live conversation opens Morning Pulse, then Stock Watch: orb compact, one socket throughout", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  const sessions = watch(page, "session")
  await page.goto("/")
  await startText(page)
  await orbAt(page, "immersive")
  await page.getByRole("button", { name: shell.backHome }).click()
  await orbAt(page, "ambient")
  await yourFlow(page)
    .getByRole("link", { name: /Morning Pulse/ })
    .click()
  await orbAt(page, "compact")
  await expect(
    page.getByRole("heading", { level: 1, name: "Morning Pulse" })
  ).toBeVisible()
  await page.getByRole("link", { name: sc.noticed.reviewStock }).click()
  await expect(
    page.getByRole("heading", { level: 1, name: "Stock Watch" })
  ).toBeVisible()
  // Scene to scene the orb stays compact (no expand/shrink) and the conversation is the same one.
  await expect(page.locator("[data-orb]")).toHaveAttribute(
    "data-orb",
    "compact"
  )
  await expect(phase(page)).toHaveText(new RegExp(shell.phase.live))
  expect(agent.connections).toBe(1)
  expect(sessions.filter((x) => x.method === "POST")).toHaveLength(1)
})

test("the agent quietly knows what the open scene shows (ids and types, never values)", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  const contexts = () =>
    agent.sent
      .filter((e) => e.type === "contextual_update")
      .map((e) => String(e.text))
      .filter((text) => text.includes("flow_context"))
      .map((text) => JSON.parse(text).flow_context)
  await page.getByRole("button", { name: shell.backHome }).click()
  await yourFlow(page).getByRole("link", { name: /Morning Pulse/ }).click()
  await expect(page).toHaveURL(/\/scenes\/pulse$/)
  await expect
    .poll(() => contexts().at(-1))
    .toEqual({
      active_scene: "pulse",
      visible: [
        { id: "sales-today", type: "metric_card", metric: "sales.gross", period: "today" },
        { id: "orders-pending", type: "metric_card", metric: "orders.pending", period: "today" },
        { id: "sales-trend", type: "line_chart", metric: "sales.gross", period: "last_7_days" },
        { id: "attention", type: "attention_panel" },
      ],
    })
  expect(JSON.stringify(contexts())).not.toMatch(/€|\d{3,}/)
})
