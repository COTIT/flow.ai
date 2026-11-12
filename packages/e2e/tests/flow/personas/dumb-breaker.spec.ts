// Persona 4, the "dumb" breaker: pastes huge text and garbage, sends empty messages, sends fifty in
// a row, taps anywhere, rotates mid-action, opens things and immediately does something else.
// SIMULATED API and voice (see harness.ts).
import { sc } from "../scenes-api"
import { v } from "../voice"
import {
  arm,
  BAD_FILES,
  EMPTYISH,
  ZERO_WIDTH,
  expectNotBroken,
  GARBAGE,
  huge,
  tapAnywhere,
} from "./dumb-breaker.helpers"
import type { Page } from "@playwright/test"

import { expectSingleScroller } from "../../../src/test"
import { expect, labels, test, type Step, openComposer, editable } from "./harness"

/** Signed in, a text conversation open and live. */
async function startTyping(page: Page) {
  await page.goto("/")
  await page.getByRole("button", { name: labels.textInstead }).click()
  await expect(page.getByRole("button", { name: labels.addFiles })).toBeVisible()
}

const KEYS = [
  "Enter", "Escape", "Tab", "Shift+Tab", "Space", "Backspace", "Delete", "ArrowUp", "ArrowDown",
  "ArrowLeft", "Home", "End", "PageDown", "ControlOrMeta+a", "ControlOrMeta+z", "F5x", "x", "?",
]

/** Steps per walk: PERSONA_STEPS wins; CI runs a short walk (suite budget), local runs the full one. */
const STEPS = Number(process.env.PERSONA_STEPS) || (process.env.CI ? 12 : 40)

test("dumb breaker: garbage, empty sends, bursts, random taps, nonsense order", async ({
  persona,
  page,
}) => {
  const dialogs = await arm(page)
  await page.emulateMedia({ colorScheme: persona.chance(0.5) ? "dark" : "light" })
  await page.goto("/")

  const box = page.getByLabel(labels.message)
  const send = page.getByRole("button", { name: labels.send })
  /** Puts text in the box without caring if it is ready: a paste (fill) or a quick insert. */
  const put = async (text: string) => {
    await openComposer(page)
    if (!(await editable(box))) return false
    if (persona.chance(0.5)) await box.fill(text, { timeout: 5_000 })
    else {
      await box.click({ timeout: 2_000 }).catch(() => {})
      await page.keyboard.insertText(text)
    }
    return true
  }
  const tray = page.getByRole("list", { name: v.attachments }).getByRole("listitem")
  const fire = async () =>
    persona.chance(0.6) ? box.press("Enter").catch(() => {}) : persona.click(send)

  const steps: Step[] = [
    {
      name: "paste garbage and send",
      weight: 6,
      run: async () => {
        const text = persona.pick(GARBAGE)
        persona.log(`   ${JSON.stringify(text.slice(0, 50))}`)
        if (await put(text)) await fire()
      },
    },
    {
      name: "paste 100k characters and send",
      weight: 1,
      run: async () => {
        if (await put(huge(persona.chance(0.5)))) await fire()
      },
    },
    {
      name: "send an empty-looking message",
      weight: 3,
      run: async () => {
        const text = persona.pick(EMPTYISH)
        persona.log(`   ${JSON.stringify(text.slice(0, 12))}`)
        await put(text)
        await fire()
      },
    },
    {
      name: "send many in a row",
      weight: 1,
      run: async () => {
        const n = persona.int(10, 50)
        persona.log(`   ×${n}`)
        await openComposer(page)
        for (let i = 0; i < n; i++) {
          if (!(await editable(box))) break
          const typed = await box
            .fill(persona.chance(0.2) ? persona.pick(GARBAGE) : `msg ${i}`, { timeout: 5_000 })
            .then(() => true, () => false)
          if (!typed) break
          await box.press("Enter").catch(() => {})
        }
      },
    },
    {
      name: "attach garbage files",
      weight: 2,
      run: async () => {
        const picker = page.getByLabel(v.chooseFiles)
        if (!(await picker.count())) return
        // Few files at a time: a full tray at 375 px is P4-2 and would hide every later break.
        if ((await tray.count()) >= 2 || (page.viewportSize()?.height ?? 0) < 800) return
        const files = Array.from({ length: persona.int(1, 3) }, () => persona.pick(BAD_FILES))
        persona.log(`   ${files.map((f) => f.name.slice(0, 16)).join(" | ")}`)
        await picker.setInputFiles(files).catch(() => {})
        if (persona.chance(0.5)) {
          await put(persona.chance(0.5) ? "what is this?" : persona.pick(GARBAGE))
          await fire()
        }
      },
    },
    { name: "tap anywhere", weight: 6, run: () => tapAnywhere(persona, page) },
    { name: "click any control", weight: 3, run: () => persona.clickAnything() },
    {
      name: "double/triple click any control",
      weight: 1,
      run: async () => {
        const controls = page
          .locator("button:visible:not([disabled])")
          .filter({ hasNotText: labels.logout })
        const n = await controls.count()
        if (n) await persona.click(controls.nth(persona.int(0, n - 1)), persona.pick(["double", "triple"] as const))
      },
    },
    {
      name: "mash keys",
      weight: 2,
      run: async () => {
        for (let i = persona.int(3, 15); i > 0; i--) {
          const key = persona.pick(KEYS)
          // Tab can land on Log out; signing out on purpose is not a break, so never press it.
          const onLogout = await page
            .evaluate((src) => {
              const el = document.activeElement as HTMLElement | null
              const name = el?.getAttribute("aria-label") ?? el?.textContent ?? ""
              return new RegExp(src).test(name.trim())
            }, labels.logout.source)
            .catch(() => false)
          if (onLogout && /Enter|Space/.test(key)) continue
          await page.keyboard.press(key === "F5x" ? "F5" : key).catch(() => {})
        }
      },
    },
    {
      name: "send then rotate / resize at once",
      weight: 2,
      run: async () => {
        await put(persona.pick(GARBAGE))
        const sizes = [[375, 812], [812, 375], [834, 1112], [1280, 800], [320, 568]] as const
        // Files and a short screen are P4-2 already: then only tall sizes, and no rotating.
        const files = (await tray.count()) > 0
        const [w, h] = persona.pick(files ? sizes.filter(([, h]) => h >= 800) : sizes)
        persona.log(`   ${w}×${h}`)
        await Promise.all([fire(), persona.chance(0.5) && !files ? persona.rotate() : persona.resize(w, h)])
      },
    },
    {
      name: "open something then do something unrelated at once",
      weight: 3,
      run: async () => {
        const [what, opener] = persona.pick([
          ["scene", () => page.getByRole("region", { name: sc.yourFlow }).getByRole("link").first()],
          ["orb", () => page.locator("[data-orb]").first()],
          ["use text", () => page.getByRole("button", { name: labels.textInstead })],
          ["storefront", () => page.getByRole("link", { name: "Storefront", exact: true })],
        ] as const)
        const [then, unrelated] = persona.pick([
          ["escape", () => persona.keys("Escape")],
          ["home", () => persona.backHome()],
          ["back", () => persona.history("back")],
          ["reload", () => persona.reload()],
          ["tap", () => tapAnywhere(persona, page)],
          ["type", () => put("what")],
        ] as const)
        persona.log(`   ${what} + ${then}`)
        await Promise.all([persona.click(opener()), unrelated()])
      },
    },
    {
      name: "agent repeats garbage back",
      weight: 2,
      run: async () => {
        const text = persona.pick(GARBAGE)
        // Own event id: rows without one merge in the client (a mock artifact, not a finding).
        persona.agent.emit({
          type: "agent_response",
          agent_response_event: {
            agent_response: persona.chance(0.3) ? text.repeat(persona.int(2, 20)) : text,
            event_id: 50_000 + persona.trail.length,
          },
        })
      },
    },
    { name: "start typing a conversation", weight: 2, run: () => persona.startText() },
    {
      name: "flip dark / light",
      weight: 1,
      run: () => page.emulateMedia({ colorScheme: persona.chance(0.5) ? "dark" : "light" }),
    },
    { name: "reload mid-whatever", weight: 1, run: () => persona.reload() },
    { name: "browser back", weight: 1, run: () => persona.history("back") },
    { name: "home", weight: 1, run: () => persona.backHome() },
  ]

  // Every step is followed by P4's own invariants (the harness checks its own after).
  await persona.run(
    steps.map((step) => ({
      ...step,
      run: async () => {
        await step.run()
        // A random tap can land on Log out (the header reflows as its status changes): the
        // person signs in again. A full load restores the seeded login.
        // A sign-out lands a moment after the tap: wait for the shell or the login form.
        await page
          .locator('[data-flow-scroll], input[type="password"]')
          .first()
          .waitFor({ timeout: 5_000 })
          .catch(() => {})
        await page.waitForTimeout(200)
        const signedOut =
          new URL(page.url()).pathname === "/login" ||
          (await page.locator('input[type="password"]').count()) > 0
        if (signedOut) {
          persona.log("   signed out → signs in again")
          await page.goto("/")
        }
        await expectNotBroken(persona, dialogs)
      },
    })),
    { count: STEPS }
  )
})

// Deterministic sweep: every garbage line once, every empty-looking line once, one 100k paste.
test("dumb breaker sweep: each garbage line shows as plain text; empty ones send nothing", async ({
  persona,
  page,
}, info) => {
  // Text rendering does not depend on width: CI sweeps once (desktop), local runs every width.
  test.skip(!!process.env.CI && info.project.name !== "flow-desktop", "CI: one width is enough")
  const dialogs = await arm(page)
  await startTyping(page)
  const box = page.getByLabel(labels.message)
  const mine = page.locator('[data-role="user"]')

  for (const text of EMPTYISH.filter((t) => t !== ZERO_WIDTH)) {
    const before = await mine.count()
    await box.fill(text)
    await box.press("Enter")
    await page.waitForTimeout(150)
    expect(await mine.count(), `empty-looking ${JSON.stringify(text.slice(0, 8))} was sent`).toBe(before)
  }
  for (const text of [...GARBAGE, huge(false), huge(true)]) {
    await box.fill(text)
    // A send already on its way keeps the box; pressing again is what a person does.
    await expect(async () => {
      await box.press("Enter")
      await expect(box).toHaveValue("", { timeout: 1_000 })
    }, `box not cleared after ${JSON.stringify(text.slice(0, 30))}`).toPass({ timeout: 10_000 })
    await expectNotBroken(persona, dialogs)
    await persona.check()
  }
})

// P4-1: a message of only zero-width characters (U+200B/C/D, U+2060) looks empty but is sent:
// an empty gold bubble appears and the agent is asked about "nothing". trim() keeps these.
test("P4-1: a zero-width-only message is not sent", async ({ persona, page }) => {
  await startTyping(page)
  const box = page.getByLabel(labels.message)
  await box.fill(ZERO_WIDTH)
  await box.press("Enter")
  await page.waitForTimeout(500)
  await expect(page.locator('[data-role="user"]')).toHaveCount(0)
  expect(persona.agent.sent.filter((e) => e.type === "user_message")).toEqual([])
})

// P4-2: at 375 px every attached file adds a row to the dock; from the 6th file on the dock is
// taller than the screen, the whole page scrolls (Flow's one-scroller layout breaks) and the
// message box and Send drop below the fold. 10 files are allowed, so 10 must fit.
test("P4-2: ten attachments at 375 px keep the page inside the screen", async ({ persona, page }) => {
  void persona
  await page.setViewportSize({ width: 375, height: 812 })
  await startTyping(page)
  const shelf = BAD_FILES.find((f) => f.name === "shelf.png")!
  await page
    .getByLabel(v.chooseFiles)
    .setInputFiles(Array.from({ length: 10 }, (_, n) => ({ ...shelf, name: `shelf-${n + 1}.png` })))
  await expect(page.getByRole("list", { name: v.attachments }).getByRole("listitem")).toHaveCount(10)
  await expectSingleScroller(page)
  await expect(page.getByRole("button", { name: labels.send })).toBeInViewport()
})
