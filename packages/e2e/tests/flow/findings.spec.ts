// Open findings from the Flow stress sweep: each test reproduces one break and is marked
// test.fail() with its id until a fix lands (then the fixer drops the mark). SIMULATED API + voice.
// Findings list: side-session notes, child-stress-test-findings.md.
import { expectNoLayoutShift, markInjected } from "../../src/test"
import { HONEY, SPICES, TEA, sc, sceneWorld } from "./scenes-api"
import { expect, test } from "./storefront-api"

test.beforeEach(({ api }) => void api)

test("SW1: a keyboard user never lands on the orb while it is scrolled out of view", async ({
  page,
}) => {
  test.fail(
    !process.env.FINDINGS_RED,
    "SW1 open: Tab reaches the orb button after main was scrolled; it stays clipped"
  )
  await page.goto("/")
  await expect(page.locator("[data-orb]")).toBeVisible()
  for (let i = 0; i < 60; i++) {
    await page.keyboard.press("Tab")
    const focusedOrb = await page.evaluate(() =>
      document.activeElement?.hasAttribute("data-orb")
    )
    if (!focusedOrb) continue
    // Focus must never be obscured (WCAG 2.4.11): the focused orb is drawn on screen.
    const shown = await page.evaluate(() => {
      const orb = document.activeElement as HTMLElement
      const box = orb.getBoundingClientRect()
      const layer = orb.parentElement!.getBoundingClientRect()
      return box.top >= layer.top - 1 && box.bottom <= layer.bottom + 1
    })
    expect(shown, "focused orb is clipped out of view").toBe(true)
    return
  }
  throw new Error("the orb was never reached by Tab")
})

for (const size of [
  { name: "phone landscape", width: 812, height: 375 },
  { name: "200% zoom on a laptop", width: 640, height: 400 },
]) {
  test(`SW2: ${size.name} leaves room for the scene itself`, async ({
    page,
  }) => {
    test.fail(
      !process.env.FINDINGS_RED,
      "SW2 open: header + scene bar + dock take almost the whole height (D1/D2)"
    )
    await page.setViewportSize({ width: size.width, height: size.height })
    await page.goto("/scenes/pulse")
    await expect(
      page.getByRole("heading", { level: 1, name: "Morning Pulse" })
    ).toBeAttached()
    const height = await page.evaluate(
      () =>
        document.querySelector("[data-flow-scroll]")!.getBoundingClientRect()
          .height
    )
    // At least a third of the screen for the content (header, scene bar, dock share the rest).
    expect(height).toBeGreaterThanOrEqual(size.height / 3)
  })
}

test("SW3: a component the API refuses (403) says it is not available for the role, at once", async ({
  page,
}) => {
  markInjected(page, /\/sales-summary/)
  await page.route(/\/sales-summary/, (route) =>
    route.fulfill({
      status: 403,
      json: { message: "This action is unauthorized." },
    })
  )
  await page.goto("/scenes/pulse")
  const card = page.locator('[data-component="sales-today"]')
  await expect(card).toContainText("Not available for your role", {
    timeout: 3_000,
  })
})

const salesCard = (page: import("@playwright/test").Page) =>
  page.locator('[data-component="sales-today"]')

test("SW3: a 403 is asked once, with no retry button and no raw code", async ({
  page,
}) => {
  let asked = 0
  markInjected(page, /\/sales-summary/)
  await page.route(/\/sales-summary/, (route) => {
    asked++
    return route.fulfill({ status: 403, json: { message: "no" } })
  })
  await page.goto("/scenes/pulse")
  await expect(salesCard(page)).toContainText("Not available for your role")
  await expect(salesCard(page).getByRole("button")).toHaveCount(0)
  await expect(salesCard(page)).not.toContainText("403")
  expect(asked).toBe(1)
})

test("SW3: a 404 says it is not available any more, without retrying", async ({
  page,
}) => {
  let asked = 0
  markInjected(page, /\/sales-summary/)
  await page.route(/\/sales-summary/, (route) => {
    asked++
    return route.fulfill({ status: 404, json: { message: "gone" } })
  })
  await page.goto("/scenes/pulse")
  await expect(salesCard(page)).toContainText("isn't available any more")
  expect(asked).toBe(1)
})

test("SW3: a 429 says when it tries again (Retry-After) and then recovers", async ({
  page,
}) => {
  let asked = 0
  markInjected(page, /\/sales-summary/)
  await page.route(/\/sales-summary/, (route) => {
    if (++asked > 1) return route.fallback()
    return route.fulfill({
      status: 429,
      headers: {
        "retry-after": "3",
        "access-control-allow-origin": "*",
        "access-control-expose-headers": "*",
      },
      json: { message: "slow down" },
    })
  })
  await page.goto("/scenes/pulse")
  await expect(salesCard(page)).toContainText("Busy, trying again in 3 seconds")
  await expect(salesCard(page)).not.toContainText("Busy", { timeout: 8_000 })
  expect(asked).toBeGreaterThan(1)
})

for (const scene of ["pulse", "orders", "sales"]) {
  test(`SW8: /scenes/${scene} loads without layout shift`, async ({ page }) => {
    test.fail(
      !process.env.FINDINGS_RED,
      "SW8 open: the dock panel under the scene grows 72 px once its suggestions load (CLS 0.02 desktop, 0.19 at 375); scene cards no longer shift"
    )
    await page.goto(`/scenes/${scene}`)
    await expect(page.locator("[data-component]").first()).toBeVisible()
    await page.waitForTimeout(1_500)
    await expectNoLayoutShift(page)
  })
}

// P2-3a: two clicks in one frame (bouncy touch screen, switch device, script) used to pass
// `disabled={isPending}` before React re-rendered, so the same write went out twice.
const twice = {
  "two clicks in one task": (b: import("@playwright/test").Locator) =>
    b.evaluate((el: HTMLElement) => {
      el.click()
      el.click()
    }),
  "clickCount 2": (b: import("@playwright/test").Locator) =>
    b.click({ clickCount: 2 }),
}

for (const [how, click] of Object.entries(twice)) {
  test(`P2-3a: Acknowledge and Snooze write once on ${how}`, async ({
    page,
    api,
  }) => {
    sceneWorld(api)
    await page.goto("/scenes/noticed")
    const stock = page.locator('[data-group="stock.below_minimum"]')
    const buttons: [string, string][] = [
      [HONEY, sc.noticed.acknowledge],
      [TEA, sc.noticed.snoozeTomorrow],
      [SPICES, sc.noticed.snoozeWeek],
    ]
    for (const [ref, name] of buttons) {
      const before = api.callsTo("PUT", "/workspace/w1/attention/marks").length
      await click(
        stock.locator(`[data-item="${ref}"]`).getByRole("button", { name })
      )
      await expect
        .poll(() => api.callsTo("PUT", "/workspace/w1/attention/marks").length)
        .toBeGreaterThan(before)
      await page.waitForTimeout(500)
      expect(
        api.callsTo("PUT", "/workspace/w1/attention/marks").length - before,
        `${name} on ${how}`
      ).toBe(1)
    }
  })
}

test("P2-3a: Pause on My Routines writes once on two clicks in one task", async ({
  page,
  api,
}) => {
  const row = {
    id: "r1",
    active: true,
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
  }
  api
    .on("GET", "/workspace/w1/scheduled-reports", () => ({ data: [row] }))
    .on("POST", "/workspace/w1/scheduled-reports/:id", ({ body }) => ({
      data: { ...row, ...(body as object) },
    }))
  await page.goto("/scenes/routines")
  await twice["two clicks in one task"](
    page.locator('[data-report="r1"]').getByRole("button", { name: sc.pause })
  )
  await page.waitForTimeout(800)
  expect(
    api.callsTo("POST", "/workspace/w1/scheduled-reports/r1")
  ).toHaveLength(1)
})
