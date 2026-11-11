// Flow layout (canvas 8AQVf7XEiV1cSyqPi5G6p9, boards Main, Conversation, Split, Tablet, Phone,
// PhoneSheet) over a SIMULATED API and voice. Runs at 1280, 834 and 375 (the three projects).
import type { Page } from "@playwright/test"

import { expectNoHorizontalScroll, expectSingleScroller } from "../../src/test"
import { expect, test } from "./storefront-api"
import { s } from "./shell"
import { fakeAgent, send, startText, v } from "./voice"

test.beforeEach(({ api }) => void api)

const header = (page: Page) => page.locator("[data-flow-header]")
const rail = (page: Page) =>
  page.getByRole("complementary", { name: s.conversation })
const sheet = (page: Page) => page.getByRole("region", { name: s.conversation })
const width = (page: Page) => page.viewportSize()?.width ?? 1280

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
    await expect(rail(page).getByRole("region", { name: s.dock })).toBeVisible()
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
    await expect(
      sheet(page).getByRole("region", { name: s.dock })
    ).toBeVisible()
    await expect(sheet(page).getByRole("log")).toHaveCount(0)
    await expectSingleScroller(page)
    await raise.click()
    const lower = page.getByRole("button", { name: s.lowerConversation })
    await expect(lower).toHaveAttribute("aria-expanded", "true")
    await expect(sheet(page).getByRole("log")).toContainText("Answer: Hello")
    await expect(
      sheet(page).getByRole("region", { name: s.dock })
    ).toBeVisible()
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
