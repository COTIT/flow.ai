// Open findings from the Flow stress sweep: each test reproduces one break and is marked
// test.fail() with its id until a fix lands (then the fixer drops the mark). SIMULATED API + voice.
// Findings list: side-session notes, child-stress-test-findings.md.
import { markInjected } from "../../src/test"
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
