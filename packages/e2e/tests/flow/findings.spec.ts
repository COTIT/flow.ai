// Open findings from the Flow stress sweep: each test reproduces one break and is marked
// test.fail() with its id until a fix lands (then the fixer drops the mark). SIMULATED API + voice.
// Findings list: side-session notes, child-stress-test-findings.md.
import {
  expectNoLayoutShift,
  expectTapTargets,
  markInjected,
} from "../../src/test"
import { rowAction } from "./lists"
import { HONEY, SPICES, TEA, sc, sceneWorld } from "./scenes-api"
import { s } from "./shell"
import { expect, test } from "./storefront-api"
import { readBack, receipt, scene, stockReview, stockRoutes } from "./task"
import { fakeAgent, startText, v, tapOrb } from "./voice"

test.beforeEach(({ api }) => void api)

test("SW1: a keyboard user never lands on the orb while it is scrolled out of view", async ({
  page,
}) => {
  await page.goto("/")
  await expect(page.locator("[data-orb]")).toBeVisible()
  for (let i = 0; i < 60; i++) {
    await page.keyboard.press("Tab")
    // Orb-first home: the orb's keyboard target is its place in the page (named "Tap to talk").
    const focusedOrb = await page.evaluate(
      (name) => document.activeElement?.getAttribute("aria-label") === name,
      s.tapToTalk
    )
    if (!focusedOrb) continue
    // Focus must never be obscured (WCAG 2.4.11): the focused orb is drawn on screen.
    const shown = await page.evaluate(() => {
      const orb = document.activeElement as HTMLElement
      const box = orb.getBoundingClientRect()
      const view = document.querySelector("[data-flow-scroll]")!.getBoundingClientRect()
      return box.top >= view.top - 1 && box.bottom <= view.bottom + 1
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

const stockReadBack = (
  api: Parameters<Parameters<typeof test>[2]>[0]["api"]
) => {
  const stock =
    readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
  api.on(
    "GET",
    "/workspace/:ws/products/:id/stock-with-bins",
    () => stock.body as object
  )
}
const propose = (agent: Awaited<ReturnType<typeof fakeAgent>>) =>
  agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 120 }] },
  })

test("SW4: after Approve from the keyboard, focus stays in the task (never lost to the page)", async ({
  page,
  api,
}) => {
  stockReadBack(api)
  const agent = await fakeAgent(page)
  await stockRoutes(page)
  await page.goto("/")
  await startText(page)
  propose(agent)
  const approve = scene(page).getByRole("button", { name: v.approve })
  await approve.focus()
  await page.keyboard.press("Enter")
  await expect(receipt(page)).toBeVisible()
  // Focus moves to what replaced the button (the receipt), not to <body>.
  await expect
    .poll(() => page.evaluate(() => document.activeElement !== document.body))
    .toBe(true)
})

// The review never takes focus from someone typing. (The dock's field itself still loses focus when
// the task scene opens: a separate layout finding, reported to the parent.)
test("SW4: a review proposed while the person types never takes their focus", async ({
  page,
  api,
}) => {
  stockReadBack(api)
  const agent = await fakeAgent(page)
  await stockRoutes(page)
  await page.goto("/")
  await startText(page)
  const message = page.getByLabel(v.message)
  await message.fill("and also")
  propose(agent)
  await expect(scene(page)).toContainText("Confirm action 135790")
  await page.waitForTimeout(300)
  await expect(page.locator("#flow-review-title")).not.toBeFocused()
})

test("SW4: a review that opens takes focus from the page", async ({
  page,
  api,
}) => {
  stockReadBack(api)
  const agent = await fakeAgent(page)
  await stockRoutes(page)
  await page.goto("/")
  await startText(page)
  await page.evaluate(() =>
    (document.activeElement as HTMLElement | null)?.blur()
  )
  propose(agent)
  await expect(page.locator("#flow-review-title")).toBeFocused()
})

test("SW5: a review that expires while open says so where the card was", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  await page.route("**/api/voice/tools", (route) => {
    const body = route.request().postDataJSON() as {
      command?: string
      operationId?: string
    }
    if (body.command !== "execute" || body.operationId !== "stock_set_levels")
      return route.fallback()
    const review = stockReview(120)
    review.review.expiresAt = new Date(Date.now() + 2_000).toISOString()
    return route.fulfill({ json: review })
  })
  await page.goto("/")
  await startText(page)
  propose(agent)
  await expect(scene(page)).toContainText("Confirm action 135790")
  // The dock already says it; the scene the person is looking at must not go blank.
  await expect(scene(page)).toContainText("This review expired", {
    timeout: 6_000,
  })
  // One way forward: asking again sends one plain request for a fresh proposal.
  await scene(page).getByRole("button", { name: "Ask again" }).dblclick()
  await expect
    .poll(() => agent.sent.filter((e) => e.type === "user_message").length)
    .toBe(1)
  await expect(scene(page)).not.toContainText("This review expired")
})

test("SW6: a refused microphone says so in plain words, never the browser's raw error", async ({
  page,
}) => {
  await page.addInitScript(() => {
    navigator.mediaDevices.getUserMedia = () =>
      Promise.reject(new DOMException("denied", "NotAllowedError"))
  })
  await fakeAgent(page)
  await page.goto("/")
  await tapOrb(page) // orb-first home: the orb starts voice
  const dock = page.getByRole("region", { name: "Ask Genesy" })
  await expect(dock.getByRole("alert")).toContainText(
    /Microphone access was refused/
  )
  await expect(dock).not.toContainText(/\bdenied\b/)
})

test("SW7: every header control is a 44 px touch target (language picker is 32 px)", async ({
  page,
}) => {
  await page.goto("/")
  await expect(page.locator("[data-flow-header]")).toBeVisible()
  await expectTapTargets(page, { within: "[data-flow-header]" })
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
    // Below 1024 px it passes or fails by load timing until d88bbcaf1 (card placeholders) is
    // merged; the dock no longer shifts. Not run there until then.
    test.fixme(
      !process.env.FINDINGS_RED && page.viewportSize()!.width < 1024,
      "SW8 open below 1024 px until d88bbcaf1 (card placeholders) is merged"
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
  "clickCount 2": async (b: import("@playwright/test").Locator) => {
    await b.click({ clickCount: 2 })
    // On a menu item the second click lands on what is under it (Snooze ▾ opens again).
    // Settled once the closing menu is gone and every open menu has its trigger expanded.
    const menus = b.page().getByRole("menu")
    const reopened = b
      .page()
      .locator('[aria-haspopup="menu"][aria-expanded="true"]')
    await expect
      .poll(async () => (await menus.count()) - (await reopened.count()))
      .toBe(0)
    if (await reopened.count()) await b.page().keyboard.press("Escape")
  },
}

for (const [how, click] of Object.entries(twice)) {
  test(`P2-3a: Acknowledge and Snooze write once on ${how}`, async ({
    page,
    api,
  }) => {
    sceneWorld(api)
    await page.goto("/scenes/noticed")
    const stock = page.locator('[data-group="stock.below_minimum"]')
    // Each control as this width shows it (inline, Snooze ▾ or the ⋯ row menu); the press that
    // writes is the doubled one.
    const actions = [
      [HONEY, "acknowledge"],
      [TEA, "tomorrow"],
      [SPICES, "week"],
    ] as const
    for (const [ref, name] of actions) {
      const before = api.callsTo("PUT", "/workspace/w1/attention/marks").length
      await rowAction(page, stock.locator(`[data-item="${ref}"]`), name, click)
      await expect
        .poll(() => api.callsTo("PUT", "/workspace/w1/attention/marks").length)
        .toBeGreaterThan(before)
      await page.waitForTimeout(500)
      expect(
        api.callsTo("PUT", "/workspace/w1/attention/marks").length - before,
        `${name} on ${how}`
      ).toBe(1)
      await page.keyboard.press("Escape") // a menu the second click left open
      await expect(page.getByRole("menu")).toHaveCount(0)
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
