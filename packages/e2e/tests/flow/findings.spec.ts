// Open findings from the Flow stress sweep: each test reproduces one break and is marked
// test.fail() with its id until a fix lands (then the fixer drops the mark). SIMULATED API + voice.
// Findings list: side-session notes, child-stress-test-findings.md.
import {
  expectNoLayoutShift,
  expectTapTargets,
  markInjected,
} from "../../src/test"
import { expect, test } from "./storefront-api"
import { readBack, receipt, scene, stockReview, stockRoutes } from "./task"
import { fakeAgent, startText, v } from "./voice"

test.beforeEach(({ api }) => void api)

test("SW1: a keyboard user never lands on the orb while it is scrolled out of view", async ({
  page,
}) => {
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
  test.fail(
    !process.env.FINDINGS_RED,
    "SW3 open: a 403 is retried (~10 s of Refreshing…) and ends as a generic error"
  )
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
  test.fail(
    !process.env.FINDINGS_RED,
    "SW4 open: the Approve button unmounts and focus falls to <body>"
  )
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

test("SW5: a review that expires while open says so where the card was", async ({
  page,
}) => {
  test.fail(
    !process.env.FINDINGS_RED,
    "SW5 open: the card vanishes; the open scene shows only 'Answers show here'"
  )
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
})

test("SW6: a refused microphone says so in plain words, never the browser's raw error", async ({
  page,
}) => {
  test.fail(
    !process.env.FINDINGS_RED,
    "SW6 open: the dock shows the exception text (e.g. 'denied')"
  )
  await page.addInitScript(() => {
    navigator.mediaDevices.getUserMedia = () =>
      Promise.reject(new DOMException("denied", "NotAllowedError"))
  })
  await fakeAgent(page)
  await page.goto("/")
  await page.getByRole("button", { name: v.start }).first().click()
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

for (const scene of ["pulse", "orders", "sales"]) {
  test(`SW8: /scenes/${scene} loads without layout shift`, async ({ page }) => {
    test.fail(
      !process.env.FINDINGS_RED && page.viewportSize()!.width < 1024,
      "SW8 open below 1024 px until d88bbcaf1 (card placeholders) is merged; the dock no longer shifts"
    )
    await page.goto(`/scenes/${scene}`)
    await expect(page.locator("[data-component]").first()).toBeVisible()
    await page.waitForTimeout(1_500)
    await expectNoLayoutShift(page)
  })
}
