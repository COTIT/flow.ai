// Persona 10, the mobile thumb user: one hand, touch only. Taps near edges and corners, swipes,
// long-presses, pulls to refresh, types with the on-screen keyboard up, rotates mid-action and
// taps while things still move. SIMULATED API and voice (see harness.ts).
import type { Page } from "@playwright/test"

import { expectTapTargets } from "../../../src/test"
import { sc } from "../scenes-api"
import { arm, expectNotBroken } from "./dumb-breaker.helpers"
import { expect, labels, test, type Step } from "./harness"
import {
  at,
  boxAboveKeyboard,
  keyboard,
  SIZES,
  thumbPoint,
  Touch,
} from "./mobile-thumb.helpers"

// A thumb user is on a touch phone: the 375 px project (touch, mobile) only.
// eslint-disable-next-line no-empty-pattern -- Playwright needs the fixtures object first
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
  /** Tall enough that the keyboard leaves room (shorter ones are P10-2). */
  const roomForKeyboard = () => size().height >= 667

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
        if (width < 360) return // 568×320 sideways has no room at all: P10-4
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
test.fail("P10-1: every header control is at least 44 px on a phone, also in landscape", async ({
  persona,
  page,
}) => {
  void persona
  await live(page, SIZES.landscape)
  await expectTapTargets(page)
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
  const b = (await page.getByRole("button", { name: labels.logout }).boundingBox())!
  const touch = await Touch.on(page)
  await touch.tap(b.x + b.width + 6, b.y + b.height / 2)
  await page.waitForTimeout(800)
  expect(signedOut, "signed out by a tap beside the button").toBe(false)
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
test.fail("P10-4: home on a small phone in landscape shows its content", async ({ persona, page }) => {
  void persona
  await page.setViewportSize({ width: 568, height: 320 })
  await page.goto("/")
  // 0 px today: "Use text instead" sits at y≈518, under the dock, and cannot be tapped.
  await expect(page.getByRole("button", { name: labels.textInstead })).toBeInViewport({ timeout: 3_000 })
  const main = await page.locator("[data-flow-scroll]").boundingBox()
  expect(main?.height ?? 0, "height left for home").toBeGreaterThanOrEqual(120)
})
