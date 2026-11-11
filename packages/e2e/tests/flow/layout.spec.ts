// Flow layout (canvas 8AQVf7XEiV1cSyqPi5G6p9, boards Main, Conversation, Split, Tablet, Phone,
// PhoneSheet) over a SIMULATED API and voice. Runs at 1280, 834 and 375 (the three projects).
import type { Locator, Page } from "@playwright/test"

import {
  expectNoHorizontalScroll,
  expectSingleScroller,
  expectTapTargets,
} from "../../src/test"
import { expect, test } from "./storefront-api"
import { revealHeader, s } from "./shell"
import { chat, fakeAgent, send, startText, v } from "./voice"

test.beforeEach(({ api }) => void api)

const header = (page: Page) => page.locator("[data-flow-header]")
const rail = (page: Page) =>
  page.getByRole("complementary", { name: s.conversation })
const sheet = (page: Page) => page.getByRole("region", { name: s.conversation })
const width = (page: Page) => page.viewportSize()?.width ?? 1280

const dock = (page: Page) => page.getByRole("region", { name: s.dock })
const composer = (page: Page) => page.getByLabel(v.message)

/** The one dock is drawn over `container`'s dock slot (it never moves in the DOM). */
async function expectDockIn(page: Page, container: Locator) {
  await expect(dock(page)).toBeVisible()
  await expect
    .poll(async () => {
      const d = (await dock(page).boundingBox())!
      const c = (await container.boundingBox())!
      return (
        d.x >= c.x - 1 &&
        d.x + d.width <= c.x + c.width + 1 &&
        d.y >= c.y - 1 &&
        d.y + d.height <= c.y + c.height + 1
      )
    })
    .toBe(true)
}

/** Talk, then go to Morning Pulse by the page's own links (the conversation stays). */
async function talkThenPulse(page: Page) {
  await fakeAgent(page, (text, a) => a.say(`Answer: ${text}`))
  await page.goto("/")
  await startText(page)
  await send(page, "Hello")
  await expect(page.getByRole("log")).toContainText("Answer: Hello")
  await page.getByRole("button", { name: s.backHome }).click()
  await page.getByRole("link", { name: /Morning Pulse/ }).click()
  await expect(page).toHaveURL(/\/scenes\/pulse$/)
}

test.describe("header (D2)", () => {
  test("floats hidden, never pushes content, and stays a landmark", async ({
    page,
  }) => {
    await page.goto("/scenes/pulse")
    const h1 = page.getByRole("heading", { level: 1 })
    await expect(h1).toBeVisible()
    await expect(header(page)).not.toHaveAttribute("data-shown")
    await expect(page.getByRole("banner")).toHaveCount(1)
    await expect(header(page)).not.toHaveAttribute("aria-hidden")
    // Content starts at the top: no reserved header row above the scene bar.
    const top = await page
      .getByRole("button", { name: s.backHome })
      .evaluate((b) => b.getBoundingClientRect().top)
    expect(top).toBeLessThan(80)
    // The scroller keeps the browser's pull-to-refresh out.
    await expect(page.locator("[data-flow-scroll]")).toHaveCSS(
      "overscroll-behavior-y",
      "contain"
    )
    await expectSingleScroller(page)
    await expectNoHorizontalScroll(page)
  })

  test("keyboard: Tab reveals it, Escape hides it and focus goes to the content", async ({
    page,
  }) => {
    await page.goto("/scenes/pulse")
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
    await page.keyboard.press("Tab")
    await expect(
      page.getByRole("link", { name: s.skipToContent })
    ).toBeFocused()
    // Touch screens have the handle next in order; then the header's own controls.
    for (
      let i = 0;
      i < 3 &&
      !(await header(page).evaluate((h) => h.contains(document.activeElement)));
      i++
    )
      await page.keyboard.press("Tab")
    await expect(header(page)).toHaveAttribute("data-shown", "true")
    const focused = page.locator(":focus")
    await expect(focused).toBeVisible()
    expect(
      await header(page).evaluate((h) => h.contains(document.activeElement))
    ).toBe(true)
    await page.keyboard.press("Escape")
    await expect(header(page)).not.toHaveAttribute("data-shown")
    await expect(page.locator("#flow-content")).toBeFocused()
  })

  test("pointer: the top edge reveals it, leaving hides it", async ({
    page,
  }) => {
    test.skip(width(page) < 1024, "desktop pointer behaviour")
    await page.goto("/scenes/pulse")
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
    await page.mouse.move(400, 2)
    await expect(header(page)).toHaveAttribute("data-shown", "true")
    await page.mouse.move(400, 600)
    await expect(header(page)).not.toHaveAttribute("data-shown")
  })

  test("touch: the handle and a swipe down from just below the top show it; a tap outside hides it", async ({
    page,
  }) => {
    test.skip(width(page) >= 1024, "touch projects")
    await page.goto("/scenes/pulse")
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
    const handle = page.getByRole("button", { name: s.showHeader })
    await expect(handle).toBeVisible()
    await handle.click()
    await expect(header(page)).toHaveAttribute("data-shown", "true")
    await expect(
      page.getByRole("button", { name: s.hideHeader })
    ).toHaveAttribute("aria-expanded", "true")
    await page.touchscreen.tap(width(page) / 2, 500)
    await expect(header(page)).not.toHaveAttribute("data-shown")
    // Option A: a swipe down starting just below the safe area.
    await page.evaluate(() => {
      const touch = (y: number) =>
        new Touch({
          identifier: 1,
          target: document.body,
          clientX: 200,
          clientY: y,
        })
      document.dispatchEvent(
        new TouchEvent("touchstart", {
          touches: [touch(12)],
          changedTouches: [touch(12)],
        })
      )
      document.dispatchEvent(
        new TouchEvent("touchend", { touches: [], changedTouches: [touch(90)] })
      )
    })
    await expect(header(page)).toHaveAttribute("data-shown", "true")
  })

  test("reduced motion: it fades, never slides", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" })
    await page.goto("/scenes/pulse")
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
    const motion = await header(page).evaluate(
      (h) => getComputedStyle(h).transitionProperty
    )
    expect(motion).toBe("opacity")
  })
})

test.describe("bento (Main, Tablet, Phone boards)", () => {
  for (const scheme of ["light", "dark"] as const)
    test(`Morning Pulse: 12 / 6 / 2 columns by results width, small and wide tiles (${scheme})`, async ({
      page,
    }) => {
      await page.emulateMedia({ colorScheme: scheme })
      await page.goto("/scenes/pulse")
      const grid = page.locator("[data-bento]")
      await expect(grid).toBeVisible()
      const columns = await grid.evaluate(
        (g) => getComputedStyle(g).gridTemplateColumns.split(" ").length
      )
      const results = await page
        .locator("[data-flow-scroll]")
        .evaluate((m) => m.clientWidth)
      expect(columns).toBe(results >= 1024 ? 12 : results >= 640 ? 6 : 2)
      await expect(page.locator('[data-size="small"]').first()).toBeVisible()
      await expectSingleScroller(page)
      await expectNoHorizontalScroll(page)
    })
})

test.describe("conversation beside the results (Split, PhoneSheet boards)", () => {
  test("desktop/tablet rail: open, resize with keys, hide and show; panes scroll on their own", async ({
    page,
  }) => {
    test.skip(width(page) < 640, "the phone gets the sheet")
    await talkThenPulse(page)
    const desktop = width(page) >= 1024
    if (!desktop) {
      // Tablet: collapsed on a scene; the bottom dock stays.
      await expect(rail(page)).toHaveCount(0)
      await expect(page.getByRole("region", { name: s.dock })).toBeVisible()
      await page.getByRole("button", { name: s.showConversation }).click()
    }
    await expect(rail(page)).toBeVisible()
    await expect(rail(page).getByRole("log")).toContainText("Answer: Hello")
    // The dock moved into the rail: still exactly one.
    await expect(page.getByRole("region", { name: s.dock })).toHaveCount(1)
    await expectDockIn(page, rail(page))
    await expectSingleScroller(page)

    const grip = page.getByRole("separator", { name: s.resizeConversation })
    const before = Number(await grip.getAttribute("aria-valuenow"))
    expect(before).toBe(desktop ? 400 : 340)
    await grip.focus()
    await page.keyboard.press("ArrowLeft")
    await expect(grip).toHaveAttribute("aria-valuenow", String(before + 20))
    // The rail grows to 520 px, less where the results would drop under 360 px.
    const max = Math.min(520, width(page) - 48 - 360)
    await expect(grip).toHaveAttribute("aria-valuemax", String(max))
    await page.keyboard.press("End")
    await expect(grip).toHaveAttribute("aria-valuenow", String(max))
    await page.keyboard.press("Home")
    await expect(grip).toHaveAttribute("aria-valuenow", "320")
    const box = await rail(page).boundingBox()
    expect(Math.round(box?.width ?? 0)).toBe(320)

    await rail(page).getByRole("button", { name: s.hideConversation }).click()
    await expect(rail(page)).toHaveCount(0)
    await expect(page.getByRole("region", { name: s.dock })).toBeVisible()
    await expectSingleScroller(page)
    await page.getByRole("button", { name: s.showConversation }).click()
    await expect(rail(page)).toBeVisible()
    await expectNoHorizontalScroll(page)
  })

  test("the rail follows the newest line; scrolled up it offers Jump to latest", async ({
    page,
  }) => {
    test.skip(width(page) < 1024, "desktop rail")
    await talkThenPulse(page)
    const log = rail(page).getByRole("log")
    for (let i = 1; i <= 8; i++) {
      await send(
        page,
        `Question ${i} ${"with a longer line of words ".repeat(4)}`
      )
      await expect(log).toContainText(`Answer: Question ${i}`)
    }
    const atBottom = () =>
      log.evaluate((l) => l.scrollHeight - l.scrollTop - l.clientHeight < 48)
    expect(await log.evaluate((l) => l.scrollHeight > l.clientHeight)).toBe(
      true
    )
    expect(await atBottom()).toBe(true)
    await expectSingleScroller(page)
    await log.evaluate((l) => {
      l.scrollTop = 0
      l.dispatchEvent(new Event("scroll"))
    })
    const jump = rail(page).getByRole("button", { name: s.jumpToLatest })
    await expect(jump).toBeVisible()
    await jump.click()
    await expect(jump).toHaveCount(0)
    expect(await atBottom()).toBe(true)
  })

  test("phone sheet: peek with the dock, raise, Escape lowers and focus returns", async ({
    page,
  }) => {
    test.skip(width(page) >= 640, "phone only")
    await talkThenPulse(page)
    const raise = page.getByRole("button", { name: s.raiseConversation })
    await expect(raise).toBeVisible()
    await expectDockIn(page, sheet(page))
    await expect(sheet(page).getByRole("log")).toHaveCount(0)
    await expectSingleScroller(page)
    await raise.click()
    const lower = page.getByRole("button", { name: s.lowerConversation })
    await expect(lower).toHaveAttribute("aria-expanded", "true")
    await expect(sheet(page).getByRole("log")).toContainText("Answer: Hello")
    await expectDockIn(page, sheet(page))
    const height = await sheet(page).evaluate(
      (el) => el.getBoundingClientRect().height
    )
    expect(height).toBeGreaterThan(page.viewportSize()!.height * 0.5)
    await expectSingleScroller(page)
    await page.keyboard.press("Escape")
    await expect(raise).toBeFocused()
    await expect(sheet(page).getByRole("log")).toHaveCount(0)
    await expectNoHorizontalScroll(page)
  })
})

test("conversation view: the orb stays pinned while the transcript scrolls (one scroller)", async ({
  page,
}) => {
  await fakeAgent(page, (text, a) =>
    a.say(`Answer: ${text} ${"more words ".repeat(20)}`)
  )
  await page.goto("/")
  await startText(page)
  for (let i = 1; i <= 6; i++) {
    await send(page, `Line ${i}`)
    await expect(page.getByRole("log")).toContainText(`Answer: Line ${i}`)
  }
  await expect(page.getByLabel(v.message)).toBeVisible()
  await expectSingleScroller(page, "[data-flow-scroll]", { strict: true })
  await expectNoHorizontalScroll(page)
})

test("SW9: the composer is one stable field: focus and draft survive a review opening, a scene change and the rail or sheet", async ({
  page,
}) => {
  const agent = await fakeAgent(page, (text, a) => a.say(`Answer: ${text}`))
  await page.goto("/")
  await startText(page)
  const field = composer(page)
  await field.fill("half a thought")
  await field.focus()
  await field.evaluate((el) => ((window as unknown as { kept: Element }).kept = el))
  const same = () =>
    field.evaluate((el) => (window as unknown as { kept: Element }).kept === el)

  // The agent proposes an action while the person is typing: the review opens beside it.
  agent.call("orders_create", { body: { reference: "Flow e2e order" } })
  await expect(page.getByRole("region", { name: chat.results })).toContainText("Flow e2e order")
  await expect(field).toBeFocused()
  await expect(field).toHaveValue("half a thought")
  expect(await same()).toBe(true)
  // Enter sends nothing while the action waits.
  await page.keyboard.press("Enter")
  await expect(field).toHaveValue("half a thought")

  // The rail or sheet opens and closes; the field is the same element with the same draft.
  const w = page.viewportSize()!.width
  if (w >= 640) {
    const toggle = page.getByRole("button", { name: s.hideConversation }).first()
    if (await toggle.isVisible()) {
      await toggle.click()
      await page.getByRole("button", { name: s.showConversation }).click()
    }
  } else {
    await page.getByRole("button", { name: s.raiseConversation }).click()
    await page.getByRole("button", { name: s.lowerConversation }).click()
  }
  await expect(field).toHaveValue("half a thought")
  expect(await same()).toBe(true)

  // A scene change: still the same field, the draft kept.
  await page.getByRole("button", { name: s.backHome }).click()
  await expect(field).toHaveValue("half a thought")
  expect(await same()).toBe(true)
  await expectSingleScroller(page)
})

test("conversing: the orb steps aside (small, in the header row) and the transcript gets the height", async ({
  page,
}) => {
  await fakeAgent(page, (text, a) => a.say(`Answer: ${text}`))
  await page.goto("/")
  await startText(page)
  await send(page, "Hello")
  await expect(page.getByRole("log")).toContainText("Answer: Hello")
  const header = page.locator("[data-conversation-header]")
  await expect(header).toBeVisible()
  // The orb glides into the header row and stays visible there, at most 56 px.
  await expect
    .poll(async () => {
      const o = (await page.locator("[data-orb]").boundingBox())!
      const h = (await header.boundingBox())!
      return o.width <= 56 && o.y >= h.y - 1 && o.y + o.height <= h.y + h.height + 1
    })
    .toBe(true)
  await expect(page.locator("[data-orb]")).toBeVisible()
  // The transcript has at least 70% of the screen above the dock.
  const room = await page.evaluate(() => {
    const main = document.querySelector("[data-flow-scroll]")!.getBoundingClientRect()
    const head = document.querySelector("[data-conversation-header]")!.getBoundingClientRect()
    const dock = document.querySelector("[data-dock-layer]")!.getBoundingClientRect()
    return (main.bottom - head.bottom) / (window.innerHeight - dock.height)
  })
  expect(room).toBeGreaterThanOrEqual(0.7)
  await expectSingleScroller(page)
  await expectNoHorizontalScroll(page)
})

for (const size of [
  { name: "phone landscape", width: 812, height: 375 },
  { name: "200% zoom on a laptop", width: 640, height: 400 },
])
  test(`SW2: on a short screen (${size.name}) the scene keeps most of the height and every control is reachable`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: size.width, height: size.height })
    await page.goto("/scenes/pulse")
    await expect(page.getByRole("heading", { level: 1, name: "Morning Pulse" })).toBeAttached()
    await expect(page.locator("[data-dock-layer]")).toBeVisible()
    const share = await page.evaluate(
      () =>
        document.querySelector("[data-flow-scroll]")!.getBoundingClientRect().height /
        window.innerHeight
    )
    expect(share).toBeGreaterThanOrEqual(0.55)
    // The dock, the way home and (on request) the header stay reachable.
    await expect(composer(page)).toBeInViewport()
    await expect(page.getByRole("button", { name: s.backHome })).toBeInViewport()
    for (
      let i = 0;
      i < 4 &&
      !(await header(page).evaluate((h) => h.contains(document.activeElement)));
      i++
    )
      await page.keyboard.press("Tab")
    await expect(header(page)).toHaveAttribute("data-shown", "true")
    await expectSingleScroller(page)
    await expectNoHorizontalScroll(page)
  })

test("P3-2: a conversation started away from home shows there (rail, or the phone sheet)", async ({
  page,
}) => {
  // Each answer is its own event, as a real agent's are.
  let event = 0
  await fakeAgent(page, (text, a) =>
    a.emit({
      type: "agent_response",
      agent_response_event: { agent_response: `Answer: ${text}`, event_id: ++event },
    })
  )
  await page.goto("/scenes/pulse")
  await expect(page.getByRole("heading", { level: 1, name: "Morning Pulse" })).toBeVisible()
  await composer(page).fill("How were sales this week?")
  await composer(page).press("Enter")
  await expect(page).toHaveURL(/\/scenes\/pulse$/)
  if (width(page) >= 640) {
    // Desktop and tablet: the rail opens beside the scene with the answer.
    await expect(rail(page)).toBeVisible()
    await expect(rail(page).getByRole("log")).toContainText(
      "Answer: How were sales this week?"
    )
    // Hidden again, a new reply is announced on its toggle.
    await rail(page).getByRole("button", { name: s.hideConversation }).click()
    await send(page, "And last week?")
    await expect(
      page.getByRole("button", { name: new RegExp(s.showConversation) })
    ).toContainText(s.newReply)
  } else {
    // Phone: the sheet peeks with the last line.
    await expect(sheet(page)).toContainText("Answer: How were sales this week?")
  }
  await expectSingleScroller(page)
})

for (const size of [
  { width: 375, height: 812 },
  { width: 812, height: 375 },
])
  test(`P4-2: ten attachments never push the dock past the screen (${size.width}×${size.height})`, async ({
    page,
  }) => {
    await page.setViewportSize(size)
    await fakeAgent(page)
    await page.goto("/")
    await startText(page)
    const png = Buffer.from(
      "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
      "base64"
    )
    await page.getByLabel(v.chooseFiles).setInputFiles(
      Array.from({ length: 10 }, (_, i) => ({
        name: `shelf-${i + 1}.png`,
        mimeType: "image/png",
        buffer: png,
      }))
    )
    await expect(page.getByRole("list", { name: v.attachments })).toContainText("shelf-10.png")
    const dockBox = (await page.locator("[data-dock-layer]").boundingBox())!
    expect(dockBox.height).toBeLessThanOrEqual(size.height * 0.5)
    await expect(composer(page)).toBeInViewport()
    await expect(page.getByRole("button", { name: v.send, exact: true })).toBeInViewport()
    await expectSingleScroller(page)
    await expectNoHorizontalScroll(page)
  })

test("P10-3: Log out asks first while a conversation is live; a near-miss tap never signs out at once", async ({
  page,
}) => {
  await fakeAgent(page, (text, a) => a.say(`Answer: ${text}`))
  await page.goto("/")
  await startText(page)
  await revealHeader(page)
  const logout = header(page).getByRole("button", { name: "Log out" })
  const box = (await logout.boundingBox())!
  // 44 px, with clear space around it.
  expect(box.height).toBeGreaterThanOrEqual(44)
  // A tap a few px off its centre (touch adjustment may snap it onto the button).
  const x = box.x + box.width / 2 + 3
  const y = box.y + box.height / 2 + 6
  if (test.info().project.use.hasTouch) await page.touchscreen.tap(x, y)
  else await page.mouse.click(x, y)
  const ask = page.getByRole("alertdialog").filter({ hasText: s.signOutTitle })
  await expect(ask).toBeVisible()
  await ask.getByRole("button", { name: s.signOutStay }).click()
  await expect(ask).toHaveCount(0)
  await expect(page).not.toHaveURL(/\/login/)
  await expect(composer(page)).toBeVisible()
})

test("P4 / P10-1: header controls keep their place when the status changes, and are 44 px on touch at any width", async ({
  page,
}) => {
  if (test.info().project.use.hasTouch) await page.setViewportSize({ width: 812, height: 375 })
  await fakeAgent(page, (text, a) => a.say(`Answer: ${text}`))
  await page.goto("/")
  await page.waitForLoadState("networkidle")
  await revealHeader(page)
  const logout = header(page).getByRole("button", { name: "Log out" })
  const before = (await logout.boundingBox())!
  await expectTapTargets(page, { within: "[data-flow-header]" })
  await startText(page)
  await revealHeader(page)
  await expect(page.locator("[data-phase]")).toHaveAttribute("data-phase", "live")
  const after = (await logout.boundingBox())!
  expect(Math.round(after.x)).toBe(Math.round(before.x))
  expect(Math.round(after.y)).toBe(Math.round(before.y))
})
