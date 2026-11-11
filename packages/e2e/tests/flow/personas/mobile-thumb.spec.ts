// Persona 10, the mobile thumb user: one hand, touch only. Taps near edges and corners, swipes,
// long-presses, pulls to refresh, types with the on-screen keyboard up, rotates mid-action and
// taps while things still move. SIMULATED API and voice (see harness.ts).
import type { Page } from "@playwright/test"

import { expectSingleScroller, expectTapTargets } from "../../../src/test"
import { sc } from "../scenes-api"
import { readBack, scene, stockRoutes } from "../task"
import { s as shell } from "../shell"
import { v as voice } from "../voice"
import { arm, expectNotBroken } from "./dumb-breaker.helpers"
import { expect, labels, test, type Step } from "./harness"
import {
  at,
  boxAboveKeyboard,
  header,
  headerHandle,
  headerShown,
  keyboard,
  sheet,
  sheetHandle,
  SIZES,
  thumbPoint,
  Touch,
} from "./mobile-thumb.helpers"

// A thumb user is on a touch phone: the 375 px project (touch, mobile) only.
test.beforeEach(({}, info) => test.skip(info.project.name !== "flow-mobile", "touch phone only"))

/** Signed in at a size, a text conversation open and live. */
async function live(page: Page, size: { width: number; height: number }) {
  await page.setViewportSize(size)
  await page.goto("/")
  await page.getByRole("button", { name: labels.textInstead }).click()
  await expect(page.getByRole("button", { name: labels.addFiles })).toBeVisible()
}

test("mobile thumb: one hand, edges, swipes, long-press, keyboard, rotation", async ({
  persona,
  page,
}) => {
  const dialogs = await arm(page)
  await page.emulateMedia({ colorScheme: persona.chance(0.5) ? "dark" : "light" })
  const start = persona.pick(Object.values(SIZES))
  await page.setViewportSize(start)
  await page.goto("/")
  const touch = await Touch.on(page)
  const box = page.getByLabel(labels.message)
  const size = () => page.viewportSize()!

  /** Within a finger of Log out: the browser would snap there (P10-3), so a thumb avoids it. */
  const nearLogout = async (x: number, y: number) => {
    const b = await page.getByRole("button", { name: labels.logout }).boundingBox({ timeout: 1_000 }).catch(() => null)
    return !!b && x > b.x - 16 && x < b.x + b.width + 16 && y > b.y - 16 && y < b.y + b.height + 16
  }
  const tapAt = async (x: number, y: number, how: "tap" | "hold" = "tap") => {
    const target = await at(page, x, y)
    persona.log(`   ${how} ${Math.round(x)},${Math.round(y)} → ${target.what}`)
    if (!target.ok || (await nearLogout(x, y))) return
    await (how === "tap" ? touch.tap(x, y) : touch.hold(x, y, persona.int(500, 1_200)))
  }
  /** A visible control's box (never Log out). */
  const someControl = async () => {
    const controls = page
      .locator('button:visible:not([disabled]), a[href]:visible:not([target="_blank"]), select:visible')
      .filter({ hasNotText: labels.logout })
    const n = await controls.count()
    return n ? controls.nth(persona.int(0, n - 1)).boundingBox({ timeout: 1_000 }).catch(() => null) : null
  }
  /** After a rotation or a new size, the shell takes the new height (--flow-vh) promptly. */
  const settledHeight = async () =>
    (await page.locator("[data-flow-scroll]").count()) &&
    expect
      .poll(
        () =>
          page.evaluate(
            () => document.documentElement.style.getPropertyValue("--flow-vh") === `${innerHeight}px`
          ),
        { message: "the shell kept the old screen height after a resize", timeout: 2_000 }
      )
      .toBe(true)
  /** P10-2 is fixed on this branch: the keyboard may come up at any size. */
  const roomForKeyboard = () => true

  const steps: Step[] = [
    {
      name: "thumb tap (edges, corners, bottom third)",
      weight: 6,
      run: async () => {
        const { x, y } = thumbPoint(persona, size().width, size().height)
        await tapAt(x, y)
      },
    },
    {
      name: "fat finger at a control's edge",
      weight: 4,
      run: async () => {
        const b = await someControl()
        if (!b) return
        const dx = persona.pick([-1, 1]) * persona.int(0, 8)
        const dy = persona.pick([-1, 1]) * persona.int(0, 8)
        const x = persona.chance(0.5) ? (dx < 0 ? b.x : b.x + b.width) + dx : b.x + b.width / 2
        const y = persona.chance(0.5) ? (dy < 0 ? b.y : b.y + b.height) + dy : b.y + b.height / 2
        await tapAt(Math.max(1, Math.min(size().width - 2, x)), Math.max(1, Math.min(size().height - 2, y)))
      },
    },
    {
      name: "long-press",
      weight: 2,
      run: async () => {
        const b = persona.chance(0.6) ? await someControl() : null
        const p = b ? { x: b.x + b.width / 2, y: b.y + b.height / 2 } : thumbPoint(persona, size().width, size().height)
        await tapAt(p.x, p.y, "hold")
      },
    },
    {
      name: "swipe a row sideways (pills, Your Flow)",
      weight: 3,
      run: async () => {
        const row = persona.pick([
          page.getByRole("list", { name: /suggest/i }),
          page.getByRole("region", { name: sc.yourFlow }).getByRole("list"),
        ])
        const b = await row.first().boundingBox({ timeout: 1_000 }).catch(() => null)
        if (!b || b.width < 40) return
        const y = b.y + b.height / 2
        const [from, to] = persona.chance(0.5) ? [b.x + b.width - 10, b.x + 10] : [b.x + 10, b.x + b.width - 10]
        persona.log(`   swipe ${Math.round(from)}→${Math.round(to)} at y=${Math.round(y)}`)
        await touch.swipe({ x: from, y }, { x: to, y }, persona.int(4, 12), persona.int(60, 400))
      },
    },
    {
      name: "scroll or pull to refresh",
      weight: 3,
      run: async () => {
        const { width, height } = size()
        const x = persona.int(20, width - 20)
        const pull = persona.chance(0.4)
        const from = pull ? persona.int(40, 120) : persona.int(Math.floor(height * 0.4), height - 120)
        const to = pull ? Math.min(height - 2, from + persona.int(150, 400)) : Math.max(2, from - persona.int(100, 400))
        persona.log(`   ${pull ? "pull" : "scroll"} ${x},${from}→${to}`)
        if (!(await nearLogout(x, from))) await touch.swipe({ x, y: from }, { x, y: to }, 10, persona.int(80, 600))
      },
    },
    {
      name: "type with the keyboard up, then send",
      weight: 3,
      run: async () => {
        if (!roomForKeyboard() || !(await box.isEditable().catch(() => false))) return
        const b = await box.boundingBox({ timeout: 1_000 }).catch(() => null)
        if (b) await tapAt(b.x + b.width / 2, b.y + b.height / 2)
        await keyboard(page, size().height > size().width ? 0.42 : 0.55)
        const { box: bottom, vh } = await boxAboveKeyboard(page)
        expect(bottom ?? 0, "the message box went under the keyboard").toBeLessThanOrEqual(vh)
        await box.fill(persona.pick(["Sales today?", "Which orders wait?", "Stock below minimum", "ok"]))
        const send = await page.getByRole("button", { name: labels.send }).boundingBox({ timeout: 1_000 }).catch(() => null)
        if (send && persona.chance(0.7)) await touch.tap(send.x + send.width / 2, send.y + send.height / 2)
        await keyboard(page, 0)
      },
    },
    {
      name: "rotate with a draft",
      weight: 3,
      run: async () => {
        const draft = persona.chance(0.5) && (await box.isEditable().catch(() => false))
        if (draft) await box.fill("half a thought")
        const { width, height } = size()
        persona.log(`   ${width}×${height} → ${height}×${width}`)
        // A short landscape with the keyboard is P10-2; rotation alone is what is tested here.
        await page.setViewportSize({ width: height, height: width })
        await settledHeight()
        if (draft) await expect(box, "the draft was lost on rotate").toHaveValue("half a thought")
      },
    },
    {
      name: "tap while things move",
      weight: 2,
      run: async () => {
        const orb = await page.locator("[data-orb]").first().boundingBox({ timeout: 1_000 }).catch(() => null)
        if (orb) await tapAt(orb.x + orb.width / 2, orb.y + orb.height / 2)
        for (let i = persona.int(1, 3); i > 0; i--) {
          await page.waitForTimeout(persona.int(20, 150))
          const { x, y } = thumbPoint(persona, size().width, size().height)
          await tapAt(x, y)
        }
      },
    },
    {
      name: "open a scene by thumb, then home",
      weight: 2,
      run: async () => {
        const link = page.getByRole("region", { name: sc.yourFlow }).getByRole("link").first()
        const b = await link.boundingBox({ timeout: 1_000 }).catch(() => null)
        if (b) await tapAt(b.x + b.width / 2, b.y + b.height / 2)
        if (persona.chance(0.5)) {
          const home = await page.getByRole("button", { name: labels.backHome }).first().boundingBox({ timeout: 1_000 }).catch(() => null)
          if (home) await tapAt(home.x + home.width / 2, home.y + home.height / 2)
        }
      },
    },
    { name: "start a text conversation", weight: 2, run: () => persona.startText() },
    {
      name: "swipe the header down / up",
      weight: 3,
      run: async () => {
        const x = persona.int(10, size().width - 10)
        if (persona.chance(0.6)) {
          const y = persona.pick([1, persona.int(4, 36), persona.int(36, 80)])
          persona.log(`   down from ${x},${y}`)
          await touch.swipe({ x, y }, { x, y: y + persona.int(40, 220) }, persona.int(3, 10), persona.int(40, 400))
        } else {
          const b = await header(page).boundingBox({ timeout: 1_000 }).catch(() => null)
          if (!b || !(await headerShown(page))) return
          persona.log(`   up on the header at x=${x}`)
          await touch.swipe({ x, y: b.y + b.height / 2 }, { x, y: Math.max(1, b.y - 60) }, 5, 120)
        }
      },
    },
    {
      name: "tap the header handle",
      weight: 2,
      run: async () => {
        const b = await headerHandle(page).boundingBox({ timeout: 1_000 }).catch(() => null)
        if (b) await tapAt(b.x + b.width / 2, b.y + b.height / 2)
      },
    },
    {
      name: "move the conversation sheet",
      weight: 3,
      run: async () => {
        const b = await sheetHandle(page).boundingBox({ timeout: 1_000 }).catch(() => null)
        if (!b) return
        const x = b.x + b.width / 2
        const y = b.y + b.height / 2
        const how = persona.pick(["tap", "up", "down", "escape"] as const)
        persona.log(`   sheet ${how}`)
        if (how === "tap") await tapAt(x, y)
        else if (how === "escape") await persona.keys("Escape")
        else await touch.swipe({ x, y }, { x, y: how === "up" ? y - persona.int(50, 300) : y + persona.int(50, 300) }, persona.int(3, 10), persona.int(40, 300))
        // The dock is always reachable, whatever the sheet does.
        await expect(page.getByLabel(labels.message)).toBeInViewport()
      },
    },
    {
      name: "change phone",
      weight: 1,
      run: async () => {
        const next = persona.pick(Object.values(SIZES))
        persona.log(`   ${next.width}×${next.height}`)
        await page.setViewportSize(next)
        await settledHeight()
      },
    },
  ]

  await persona.run(
    steps.map((step) => ({
      ...step,
      run: async () => {
        await step.run()
        await page
          .locator('[data-flow-scroll], input[type="password"]')
          .first()
          .waitFor({ timeout: 5_000 })
          .catch(() => {})
        if (await page.locator('input[type="password"]').count()) {
          persona.log("   signed out → signs in again")
          await page.goto("/")
        }
        await expectNotBroken(persona, dialogs)
      },
    }))
  )
})

// P10-1: header controls under 44 px on a phone. Every size: the Language select (122×32);
// landscape (812×375) also Storefront (94×34), Switch light or dark mode (40×40), Log out (96×34).
test("P10-1: every header control is at least 44 px on a phone, also in landscape", async ({
  persona,
  page,
}) => {
  void persona
  await live(page, SIZES.landscape)
  await headerHandle(page).tap()
  await expect.poll(() => headerShown(page)).toBe(true)
  await page.waitForTimeout(400) // it slides in
  await expectTapTargets(page, { within: "[data-flow-header]" })
})

// P10-2: the on-screen keyboard covers the message box on short screens. The shell sizes itself
// to the visual viewport, but header + dock are taller than what is left, so the dock is pushed
// below the keyboard: at 320×568 (keyboard ~42 %) and in landscape (~55 %) you type blind.
for (const [name, share] of [["se", 0.42], ["landscape", 0.55]] as const)
  test(`P10-2: with the keyboard up the message box stays visible (${name})`, async ({
    persona,
    page,
  }) => {
    void persona
    await live(page, SIZES[name])
    await page.getByLabel(labels.message).click()
    await keyboard(page, share)
    await expect
      .poll(async () => {
        const { box, vh } = await boxAboveKeyboard(page)
        return (box ?? Infinity) <= vh
      }, { message: "the message box is under the keyboard" })
      .toBe(true)
  })

// P10-3: a thumb that lands 6 px beside Log out signs out (the browser snaps a near-miss touch to
// the nearest button), with no confirmation, ending a live conversation.
test("P10-3: a near-miss tap beside Log out does not sign out", async ({ persona, page }) => {
  void persona
  await live(page, SIZES.phone)
  let signedOut = false
  page.on("request", (r) => {
    if (new URL(r.url()).pathname === "/v1/logout") signedOut = true
  })
  const touch = await Touch.on(page)
  await headerHandle(page).tap()
  await expect.poll(() => headerShown(page)).toBe(true)
  await page.waitForTimeout(400)
  const b = (await page.getByRole("button", { name: labels.logout }).boundingBox())!
  await touch.tap(b.x + b.width + 6, b.y + b.height / 2)
  await page.waitForTimeout(800)
  expect(signedOut, "signed out by a tap beside the button").toBe(false)
  // The near miss may still reach Log out, but while live it only asks: cancel it.
  const confirm = page.getByRole("alertdialog")
  if (await confirm.isVisible()) await confirm.getByRole("button", { name: /cancel|keep|stay/i }).first().tap()
  expect(signedOut).toBe(false)
  await expect(page.getByRole("button", { name: labels.addFiles })).toBeVisible()
})

// P10-4: a phone turned sideways has no room left for content: header, scene bar and dock take
// the whole height. The scroll root is 0 px tall on a scene at 740×360 (3 px at 812×375, 15 px
// at 667×375), ~50-67 px at home; at 568×320 the page itself overflows.
test("P10-4: a scene on a phone in landscape shows its content", async ({ persona, page }) => {
  void persona
  await page.setViewportSize({ width: 740, height: 360 })
  await page.goto("/scenes/pulse")
  await expect(page.getByRole("region", { name: labels.dock })).toBeVisible()
  const main = await page.locator("[data-flow-scroll]").boundingBox()
  expect(main?.height ?? 0, "height left for the scene").toBeGreaterThanOrEqual(120)
})
test("P10-4: home on a small phone in landscape shows its content", async ({ persona, page }) => {
  void persona
  await page.setViewportSize({ width: 568, height: 320 })
  await page.goto("/")
  // Was 0 px: "Use text instead" sat at y≈518 under the dock and could not be reached.
  const useText = page.getByRole("button", { name: labels.textInstead })
  await useText.scrollIntoViewIfNeeded()
  await expect(useText).toBeInViewport({ timeout: 3_000 })
  const main = await page.locator("[data-flow-scroll]").boundingBox()
  expect(main?.height ?? 0, "height left for home").toBeGreaterThanOrEqual(120)
})

// P10-2 (375×667): one extra dock line is enough. "Switch to voice" without a microphone leaves
// "Voice could not start…" in the dock; with the keyboard up the box is then 20 px under it.
test("P10-2: with the keyboard up the message box stays visible (375×667, dock notice)", async ({
  persona,
  page,
}) => {
  void persona
  await live(page, SIZES.small)
  await page.getByRole("button", { name: shell.switchVoice }).click()
  await expect(page.getByRole("region", { name: labels.dock })).toContainText(voice.upgradeFailed)
  await page.getByLabel(labels.message).click()
  await keyboard(page, 0.42)
  const { box, vh } = await boxAboveKeyboard(page)
  expect(box ?? Infinity, "the message box is under the keyboard").toBeLessThanOrEqual(vh)
})

// ---- Touch gestures of the floating header and the phone sheet (flow/stress-int-1) ------------

const GESTURE_SIZES = { phone: SIZES.phone, se: SIZES.se, landscape: SIZES.landscape } as const
/** Settled where it should be: shown and on screen, or hidden. */
async function expectHeader(page: Page, shown: boolean) {
  await expect.poll(() => headerShown(page), { message: `header ${shown ? "shown" : "hidden"}` }).toBe(shown)
  if (shown)
    await expect.poll(async () => Math.round((await header(page).boundingBox())?.y ?? -999)).toBeGreaterThanOrEqual(0)
}

for (const [name, size] of Object.entries(GESTURE_SIZES)) {
  test(`gestures ${name}: swipe below the top reveals, the very top edge does not, swipe up hides`, async ({
    persona,
    page,
  }) => {
    // P10-6: while a conversation is live the content is scrolled to its newest line on short
    // screens, so "at the top" is false and the reveal swipe is ignored (the handle still works).
    test.fail(name !== "phone", "P10-6")
    void persona
    await live(page, size)
    const touch = await Touch.on(page)
    const x = Math.round(size.width * 0.3)
    // The very top edge belongs to the OS (notification shade): no reveal from y = 0..2.
    await touch.swipe({ x, y: 1 }, { x, y: 160 })
    await page.waitForTimeout(300)
    expect(await headerShown(page), "a swipe from the very top edge revealed the header").toBe(false)
    // Just below the safe area: revealed.
    await touch.swipe({ x, y: 14 }, { x, y: 160 })
    await expectHeader(page, true)
    // Swipe up on it: hidden.
    const b = (await header(page).boundingBox())!
    await touch.swipe({ x, y: b.y + b.height / 2 }, { x, y: Math.max(1, b.y + b.height / 2 - 90) })
    await expectHeader(page, false)
    await expectSingleScroller(page)
  })

  test(`P10-5 gestures ${name}: the handle shows and hides the header by tap`, async ({ persona, page }) => {
    // P10-5: on touch the second tap never hides it: touchstart on the handle (outside the header)
    // hides it, then the click toggles it straight back (only mousedown skips the handle).
    test.fail(true, "P10-5")
    void persona
    await live(page, size)
    const touch = await Touch.on(page)
    const tapHandle = async () => {
      const b = (await headerHandle(page).boundingBox())!
      await touch.tap(b.x + b.width / 2, b.y + b.height / 2)
    }
    await tapHandle()
    await expectHeader(page, true)
    await page.waitForTimeout(300)
    await tapHandle()
    await expectHeader(page, false)
  })

  test(`gestures ${name}: fast repeated swipes end in a settled header`, async ({ persona, page }) => {
    test.fail(name === "se", "P10-6")
    void persona
    await live(page, size)
    const touch = await Touch.on(page)
    const x = Math.round(size.width * 0.3)
    for (let i = 0; i < 6; i++) {
      await touch.swipe({ x, y: 14 }, { x, y: 140 }, 4, 40)
      const b = await header(page).boundingBox()
      if (b && i % 2) await touch.swipe({ x, y: b.y + b.height / 2 }, { x, y: 2 }, 4, 40)
    }
    await touch.swipe({ x, y: 14 }, { x, y: 140 }, 4, 40)
    await expectHeader(page, true)
    // Touch: it hides by itself after a while without use.
    await expectHeader(page, true)
    await expect.poll(() => headerShown(page), { timeout: 9_000 }).toBe(false)
  })

  test(`gestures ${name}: a pull down in the content neither refreshes nor reveals`, async ({ persona, page }) => {
    void persona
    await live(page, size)
    let loads = 0
    page.on("load", () => loads++)
    const touch = await Touch.on(page)
    const x = Math.round(size.width / 2)
    await touch.swipe({ x, y: Math.round(size.height * 0.3) }, { x, y: Math.round(size.height * 0.85) }, 10, 300)
    await page.waitForTimeout(500)
    expect(loads, "the page reloaded").toBe(0)
    expect(await headerShown(page), "a pull in the content revealed the header").toBe(false)
    // A real phone pulls to refresh when the overscroll chains to the page: it must stop at Flow.
    const chain = await page.evaluate(() => ({
      html: getComputedStyle(document.documentElement).overscrollBehaviorY,
      body: getComputedStyle(document.body).overscrollBehaviorY,
      main: getComputedStyle(document.querySelector("[data-flow-scroll]")!).overscrollBehaviorY,
    }))
    expect(
      [chain.html, chain.body, chain.main].some((v) => v === "none" || v === "contain"),
      `overscroll chains to the page (pull-to-refresh): ${JSON.stringify(chain)}`
    ).toBe(true)
  })

  test(`gestures ${name}: Log out while live asks first`, async ({ persona, page }) => {
    void persona
    await live(page, size)
    let signedOut = false
    page.on("request", (r) => {
      if (new URL(r.url()).pathname === "/v1/logout") signedOut = true
    })
    const touch = await Touch.on(page)
    const h = (await headerHandle(page).boundingBox())!
    await touch.tap(h.x + h.width / 2, h.y + h.height / 2)
    await expectHeader(page, true)
    await page.waitForTimeout(300)
    const b = (await page.getByRole("button", { name: labels.logout }).boundingBox())!
    await touch.tap(b.x + b.width / 2, b.y + b.height / 2)
    const confirm = page.getByRole("alertdialog")
    await expect(confirm).toBeVisible()
    expect(signedOut, "signed out without asking").toBe(false)
    await confirm.getByRole("button", { name: /cancel|keep|stay/i }).first().tap()
    await expect(confirm).toBeHidden()
    expect(signedOut).toBe(false)
    await expect(page.getByRole("button", { name: labels.addFiles })).toBeVisible()
  })
}

/** On a phone in portrait the conversation becomes a sheet once you leave the full view. */
async function withSheet(page: Page, size: { width: number; height: number }) {
  await live(page, size)
  await page.getByLabel(labels.message).fill("Sales today?")
  await page.getByLabel(labels.message).press("Enter")
  await page.getByRole("button", { name: labels.backHome }).first().click()
  await expect(sheetHandle(page)).toBeVisible()
}
const sheetOpen = async (page: Page) =>
  (await sheetHandle(page).getAttribute("aria-expanded")) === "true"
async function expectDockReachable(page: Page) {
  await expect(page.getByLabel(labels.message)).toBeInViewport()
  await expect(page.getByRole("button", { name: labels.send })).toBeInViewport()
}

for (const [name, size] of Object.entries({ phone: SIZES.phone, se: SIZES.se })) {
  test(`sheet ${name}: handle tap, swipe up and down, Escape; the dock stays reachable`, async ({
    persona,
    page,
  }) => {
    // P10-7: at 320×568 the home orb floats over the raised sheet and covers its handle: a swipe
    // there starts on the orb, the browser pans (pointercancel) and the sheet never lowers.
    test.fail(name === "se", "P10-7")
    void persona
    await withSheet(page, size)
    const touch = await Touch.on(page)
    const handle = async () => (await sheetHandle(page).boundingBox())!
    expect(await sheetOpen(page)).toBe(false)
    await expectDockReachable(page)
    let b = await handle()
    await touch.tap(b.x + b.width / 2, b.y + b.height / 2)
    await expect.poll(() => sheetOpen(page), { message: "tap raises" }).toBe(true)
    await expectDockReachable(page)
    b = await handle()
    await touch.swipe({ x: b.x + b.width / 2, y: b.y + b.height / 2 }, { x: b.x + b.width / 2, y: b.y + 200 })
    await expect.poll(() => sheetOpen(page), { message: "swipe down lowers" }).toBe(false)
    b = await handle()
    await touch.swipe({ x: b.x + b.width / 2, y: b.y + b.height / 2 }, { x: b.x + b.width / 2, y: b.y - 200 })
    await expect.poll(() => sheetOpen(page), { message: "swipe up raises" }).toBe(true)
    const h = (await sheet(page).boundingBox())!.height
    expect(Math.abs(h - size.height * 0.62) < size.height * 0.08, `raised sheet is ~62 %: ${h}`).toBe(true)
    await page.keyboard.press("Escape")
    await expect.poll(() => sheetOpen(page), { message: "Escape lowers" }).toBe(false)
    await expectDockReachable(page)
    await expectSingleScroller(page)
  })

  test(`sheet ${name}: fast repeated swipes on the handle settle`, async ({ persona, page }) => {
    // P10-7: at 320×568 the home orb floats over the raised sheet and covers its handle: a swipe
    // there starts on the orb, the browser pans (pointercancel) and the sheet never lowers.
    test.fail(name === "se", "P10-7")
    void persona
    await withSheet(page, size)
    const touch = await Touch.on(page)
    for (let i = 0; i < 8; i++) {
      const b = (await sheetHandle(page).boundingBox())!
      const dy = i % 2 ? 160 : -160
      await touch.swipe({ x: b.x + b.width / 2, y: b.y + b.height / 2 }, { x: b.x + b.width / 2, y: b.y + dy }, 4, 30)
    }
    await page.waitForTimeout(500)
    expect(await sheetOpen(page)).toBe(false)
    await expectDockReachable(page)
    await expectSingleScroller(page)
  })

  test(`sheet ${name}: rotating while it is raised keeps the dock and one scroller`, async ({
    persona,
    page,
  }) => {
    void persona
    await withSheet(page, size)
    await sheetHandle(page).tap()
    await expect.poll(() => sheetOpen(page)).toBe(true)
    await page.getByLabel(labels.message).fill("half a thought")
    await page.setViewportSize({ width: size.height, height: size.width })
    await page.waitForTimeout(500)
    await expectDockReachable(page)
    await expect(page.getByLabel(labels.message)).toHaveValue("half a thought")
    await expectSingleScroller(page)
    await page.setViewportSize(size)
    await page.waitForTimeout(500)
    await expectDockReachable(page)
    await expectSingleScroller(page)
  })
}

for (const [name, size] of Object.entries(GESTURE_SIZES)) {
  test(`review ${name}: swipes during an open review leave it reachable and decided once`, async ({
    persona,
    page,
    api,
  }) => {
    const stock = readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
    api.on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => stock.body as object)
    await stockRoutes(page)
    await live(page, size)
    persona.agent.call("stock_set_levels", {
      path: { product: "p-500" },
      body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 120 }] },
    })
    const approve = scene(page).getByRole("button", { name: labels.approve })
    await expect(approve).toBeVisible()
    const touch = await Touch.on(page)
    const x = Math.round(size.width * 0.3)
    // Reveal and hide the header, pull in the content, swipe the sheet if there is one, rotate.
    await touch.swipe({ x, y: 14 }, { x, y: 160 }, 6, 120)
    await touch.swipe({ x, y: 120 }, { x, y: 10 }, 6, 120)
    await touch.swipe({ x, y: Math.round(size.height * 0.4) }, { x, y: Math.round(size.height * 0.8) }, 8, 200)
    const handle = await sheetHandle(page).boundingBox({ timeout: 1_000 }).catch(() => null)
    if (handle)
      await touch.swipe(
        { x: handle.x + handle.width / 2, y: handle.y + handle.height / 2 },
        { x: handle.x + handle.width / 2, y: handle.y - 150 },
        6,
        120
      )
    await page.setViewportSize({ width: size.height, height: size.width })
    await page.waitForTimeout(400)
    await page.setViewportSize(size)
    await page.waitForTimeout(400)
    // Still there, reachable by a thumb, nothing on top of it.
    await approve.scrollIntoViewIfNeeded()
    await expect(approve).toBeInViewport()
    const b = (await approve.boundingBox())!
    const onTop = await page.evaluate(
      ([x, y]) => document.elementFromPoint(x!, y!)?.closest("button")?.textContent?.trim() ?? "",
      [b.x + b.width / 2, b.y + b.height / 2]
    )
    expect(onTop, "something covers Approve").toMatch(labels.approve)
    await touch.tap(b.x + b.width / 2, b.y + b.height / 2)
    await touch.tap(b.x + b.width / 2, b.y + b.height / 2)
    await expect(approve).toBeHidden()
    persona.expectNoDuplicateWrites()
    await expectSingleScroller(page)
  })
}
