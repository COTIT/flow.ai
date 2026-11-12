import type { BrowserContext, Locator, Page, Request } from "@playwright/test"

import {
  expect,
  expectSingleScroller,
  markInjected,
  watchProblems,
} from "../../../src/test"
import type { FakeApi } from "../../office/fake-api"
import { test as flowTest } from "../storefront-api"
import commonEn from "../../../../../packages/i18n/messages/en.json" with { type: "json" }
import commonNl from "../../../../../packages/i18n/messages/nl.json" with { type: "json" }
import commonSw from "../../../../../packages/i18n/messages/sw.json" with { type: "json" }
import voiceEn from "../../../../../packages/i18n/messages/voice/en.json" with { type: "json" }
import voiceNl from "../../../../../packages/i18n/messages/voice/nl.json" with { type: "json" }
import voiceSw from "../../../../../packages/i18n/messages/voice/sw.json" with { type: "json" }
import shellEn from "../../../../../apps/flow/messages/en/shell.json" with { type: "json" }
import shellNl from "../../../../../apps/flow/messages/nl/shell.json" with { type: "json" }
import shellSw from "../../../../../apps/flow/messages/sw/shell.json" with { type: "json" }
import { fakeAgent, type Agent } from "../voice"

/**
 * SIMULATED users for Flow (mock API + mock voice behind Flow's real proxy). A persona is a seeded
 * random walk over realistic actions; after every step the invariants below are checked. A failure
 * prints the seed and the step trail, so `PERSONA_SEED=<seed>` replays it exactly.
 */

const LOCALES = [
  { common: commonEn.common, voice: voiceEn.voice, shell: shellEn.shell },
  { common: commonNl.common, voice: voiceNl.voice, shell: shellNl.shell },
  { common: commonSw.common, voice: voiceSw.voice, shell: shellSw.shell },
]
type Messages = (typeof LOCALES)[number]
const escape = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
/** One label in every Flow language, exactly (a persona may switch the language at any time). */
const inAnyLocale = (pick: (m: Messages) => string) =>
  new RegExp(`^(${[...new Set(LOCALES.map(pick))].map(escape).join("|")})$`)

/** The controls and regions personas use, matched in any language Flow speaks. */
export const labels = {
  logout: inAnyLocale((m) => m.common.logout),
  dock: inAnyLocale((m) => m.shell.dock),
  backHome: inAnyLocale((m) => m.shell.backHome),
  message: inAnyLocale((m) => m.voice.message),
  send: inAnyLocale((m) => m.voice.send),
  // Home's way to start typing (orb-first home: "Type instead").
  textInstead: inAnyLocale((m) => m.shell.typeInstead),
  continueConversation: inAnyLocale((m) => m.shell.continueConversation),
  addFiles: inAnyLocale((m) => m.voice.addFiles),
  approve: inAnyLocale((m) => m.voice.approve),
}
/** Visible and editable: isEditable() alone is true for a hidden field, so a fill would wait forever. */
export const editable = async (field: Locator) =>
  (await field.isVisible().catch(() => false)) && (await field.isEditable().catch(() => false))

/**
 * Orb-first home has no message box: before typing, a person opens the conversation the way home
 * offers it ("Type instead", or the orb while a call runs). Anywhere with a message box: nothing.
 */
export async function openComposer(page: Page) {
  const box = page.getByLabel(labels.message).first()
  if (await box.isVisible().catch(() => false)) return
  const type = page.getByRole("button", { name: labels.textInstead }).first()
  const orb = page.getByRole("button", { name: labels.continueConversation }).first()
  if (await type.isVisible().catch(() => false)) await type.click({ timeout: 2_000 }).catch(() => {})
  // While a call runs the orb continues it (tapped where it is drawn); idle, it would start voice.
  else if (await orb.isVisible().catch(() => false))
    await page.locator('[data-orb="ambient"]').click({ timeout: 2_000 }).catch(() => {})
  else return
  await box.waitFor({ state: "visible", timeout: 2_000 }).catch(() => {})
}

/** Never clicked by a random step: leaving the signed-in app ends the walk. */
const LEAVE = new RegExp(labels.logout.source.replace(/^\^|\$$/g, ""), "i")

/** mulberry32: small, fast, good enough to replay a walk. */
export function rng(seed: number) {
  let a = seed >>> 0
  return () => {
    a = (a + 0x6d2b79f5) >>> 0
    let t = Math.imul(a ^ (a >>> 15), 1 | a)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

/**
 * The run's seed: PERSONA_SEED; else 1 in CI (a red CI run always replays); else a fresh one
 * locally, to explore. Always printed and attached to the report.
 */
export function seedOf(): number {
  const fixed = Number(process.env.PERSONA_SEED)
  if (Number.isInteger(fixed) && fixed > 0) return fixed
  return process.env.CI ? 1 : Math.floor(Math.random() * 2 ** 31) + 1
}

/** Every approval decision the browser sent (from the mock's view), added to `seen`. */
export function confirmLog(
  page: Page,
  seen: { ticket: string; decision: string }[] = []
) {
  page.on("request", (request: Request) => {
    if (new URL(request.url()).pathname !== "/api/voice/confirm") return
    try {
      const body = request.postDataJSON() as {
        ticket?: unknown
        decision?: unknown
      }
      seen.push({
        ticket: String(body.ticket),
        decision: String(body.decision),
      })
    } catch {
      seen.push({ ticket: "?", decision: "unreadable" })
    }
  })
  return seen
}

/**
 * A fast, honest agent: answers each typed line (with or without files) a moment later, with its
 * own event id; refuses to connect while the network is down.
 */
export function honestAgent(page: Page, net: { down: boolean }) {
  let id = 0
  return fakeAgent(
    page,
    (text, a) =>
      setTimeout(
        () =>
          a.emit({
            type: "agent_response",
            agent_response_event: {
              agent_response: `Noted: ${text.slice(0, 60)}`,
              event_id: ++id,
            },
          }),
        80
      ),
    { refuse: () => net.down, files: true }
  )
}

/** Things that load or think; each must settle (or say what is wrong) within `ms`. */
const BUSY = '[data-slot="skeleton"], [aria-busy="true"], .animate-spin'

export type Step = { name: string; weight: number; run: () => Promise<unknown> }

export type PersonaOptions = {
  /** Text of another workspace or login that must never show (foreign data). */
  foreign?: string[]
  /** How long anything may stay busy before it counts as stuck. */
  settleMs?: number
}

/**
 * One simulated user on one page (more pages via `secondTab`). Actions are small and honest:
 * they use the UI as a person would (roles, labels), never app internals.
 */
export class Persona {
  readonly random: () => number
  readonly trail: string[] = []
  readonly confirms: { ticket: string; decision: string }[]
  readonly extraProblems: string[][] = []
  readonly pages: Page[]
  readonly agents: Agent[]

  constructor(
    readonly page: Page,
    readonly agent: Agent,
    readonly seed: number,
    readonly options: PersonaOptions = {},
    /** Shared with the fake agents: while down they refuse new connections. */
    readonly net: { down: boolean } = { down: false }
  ) {
    this.random = rng(seed)
    this.confirms = confirmLog(page)
    this.pages = [page]
    this.agents = [agent]
  }

  // ---- randomness --------------------------------------------------------------------------
  chance(p: number) {
    return this.random() < p
  }
  int(min: number, max: number) {
    return min + Math.floor(this.random() * (max - min + 1))
  }
  pick<T>(items: readonly T[]): T {
    return items[Math.floor(this.random() * items.length)]!
  }

  // ---- actions -----------------------------------------------------------------------------
  log(step: string) {
    this.trail.push(step)
  }
  /** Clicks if it is there and usable; a person clicks what they see, nothing else. */
  async click(target: Locator, how: "single" | "double" | "triple" = "single") {
    const el = target.first()
    if (!(await el.isVisible().catch(() => false))) return false
    if (!(await el.isEnabled({ timeout: 1_000 }).catch(() => false)))
      return false
    const clickCount = how === "single" ? 1 : how === "double" ? 2 : 3
    await el.click({ clickCount, timeout: 2_000 }).catch(() => {})
    return true
  }
  /**
   * A random visible control in the frame: buttons and links, never ones leaving Flow. A control
   * that vanishes mid-step (the page moved on) makes the step a no-op, never a hang.
   */
  async clickAnything(
    scope: Locator = this.page.locator("body"),
    avoid = LEAVE
  ) {
    const controls = scope.locator(
      'button:visible:not([disabled]), a[href]:visible:not([target="_blank"])'
    )
    const count = await controls.count().catch(() => 0)
    if (!count) return null
    const el = controls.nth(this.int(0, count - 1))
    const quick = { timeout: 1_000 }
    const label = await el.getAttribute("aria-label", quick).catch(() => null)
    const text = label ?? (await el.innerText(quick).catch(() => null))
    if (text === null) return null // gone before it could be read
    const name = text.trim().slice(0, 40)
    if (avoid.test(name)) return null
    await el.click({ timeout: 2_000 }).catch(() => {})
    return name
  }
  /** Types into the dock and sends (Enter or the Send button). */
  async say(
    text: string,
    how: "enter" | "button" = this.chance(0.5) ? "enter" : "button"
  ) {
    await openComposer(this.page)
    const box = this.page.getByLabel(labels.message)
    if (!(await box.isEditable({ timeout: 1_000 }).catch(() => false)))
      return false
    // The dock may turn inert (a review opened) between the check and the typing: skip, never hang.
    if (
      !(await box.fill(text, { timeout: 2_000 }).then(
        () => true,
        () => false
      ))
    )
      return false
    if (how === "enter")
      await box.press("Enter", { timeout: 2_000 }).catch(() => {})
    else await this.click(this.page.getByRole("button", { name: labels.send }))
    return true
  }
  async startText() {
    return this.click(
      this.page.getByRole("button", { name: labels.textInstead })
    )
  }
  async backHome() {
    return this.click(this.page.getByRole("button", { name: labels.backHome }))
  }
  async keys(...keys: string[]) {
    for (const key of keys) await this.page.keyboard.press(key)
  }
  async tab(times = 1, back = false) {
    for (let i = 0; i < times; i++)
      await this.page.keyboard.press(back ? "Shift+Tab" : "Tab")
  }
  /** Back or Forward, only within Flow's own history (never back out to a blank tab). */
  async history(direction: "back" | "forward") {
    const can = await this.page
      .evaluate((back) => {
        const nav = (
          window as unknown as {
            navigation?: { canGoBack: boolean; canGoForward: boolean }
          }
        ).navigation
        return back ? nav?.canGoBack : nav?.canGoForward
      }, direction === "back")
      .catch(() => false)
    if (!can) return
    await this.navigate(direction, (nav) =>
      direction === "back" ? this.page.goBack(nav) : this.page.goForward(nav)
    )
  }
  /** A reload while offline would only show the browser's error page: skipped, not a break. */
  async reload() {
    if (this.net.down) return this.log("  (reload skipped: offline)")
    await this.navigate("reload", (nav) => this.page.reload(nav))
  }
  /**
   * Back, Forward or reload, bounded: waits for the document only (not every resource); a
   * navigation that does not land in time is noted in the trail and the walk goes on.
   */
  private async navigate(
    what: string,
    go: (nav: {
      waitUntil: "domcontentloaded"
      timeout: number
    }) => Promise<unknown>
  ) {
    await go({ waitUntil: "domcontentloaded", timeout: 10_000 }).catch(
      (error: unknown) => {
        if (String(error).includes("Timeout"))
          this.log(`  (${what} did not land in 10 s)`)
      }
    )
  }
  /**
   * The network goes down (or comes back) for every tab, mocks included: setOffline alone lets
   * Playwright-routed requests and the mocked voice socket through, so while down every routed
   * request is aborted, open agent sockets drop and new ones are refused.
   */
  async offline(context: BrowserContext, on: boolean) {
    this.net.down = on
    if (on) {
      this.wentOffline = true
      for (const page of this.pages) {
        markInjected(page, /./)
        await this.tunnel(page)
      }
      for (const agent of this.agents) agent.close()
    }
    await context.setOffline(on)
    // Back online: a tab that showed the browser's offline error page loads Flow again.
    if (!on)
      for (const page of this.pages)
        if (!page.isClosed() && page.url().startsWith("chrome-error://"))
          await page
            .reload({ waitUntil: "domcontentloaded", timeout: 10_000 })
            .catch(() =>
              this.log("  (reload after offline did not land in 10 s)")
            )
  }
  private wentOffline = false
  private tunnels = new WeakSet<Page>()
  /** Registered last, so it is asked first: aborts while the network is down. */
  private async tunnel(page: Page) {
    if (this.tunnels.has(page)) return
    this.tunnels.add(page)
    await page.route("**/*", (route) =>
      this.net.down ? route.abort("internetdisconnected") : route.fallback()
    )
  }
  /** Answers one API path with a failure on purpose; the guard lets exactly that through. */
  async failApi(path: RegExp, status = 503, times = 1) {
    markInjected(this.page, path)
    let left = times
    await this.page.route(path, (route) =>
      left-- > 0
        ? route.fulfill({ status, json: { message: "Injected failure." } })
        : route.fallback()
    )
  }
  async resize(width: number, height: number) {
    await this.page.setViewportSize({ width, height })
  }
  /** Portrait ↔ landscape. */
  async rotate() {
    const size = this.page.viewportSize()
    if (size)
      await this.page.setViewportSize({
        width: size.height,
        height: size.width,
      })
  }
  /** The person talks over the agent: an interruption, then their words. */
  bargeIn(words: string, id = this.int(1000, 9999)) {
    this.agent.emit({
      type: "interruption",
      interruption_event: { event_id: id },
    })
    this.agent.emit({
      type: "user_transcript",
      user_transcription_event: { user_transcript: words, event_id: id + 1 },
    })
  }
  /** The same login in another tab, with its own guard, API and agent. */
  async secondTab(api: FakeApi) {
    const page = await this.page.context().newPage()
    this.extraProblems.push(watchProblems(page, {}, undefined))
    // A tab opened after the network dropped may still see those failures.
    if (this.wentOffline) markInjected(page, /./)
    await page.route("**/v1/**", (route) => api.handle(route))
    // Answers like the first tab's agent; its approvals count in the same log.
    const agent = await honestAgent(page, this.net)
    confirmLog(page, this.confirms)
    this.pages.push(page)
    this.agents.push(agent)
    if (this.wentOffline) await this.tunnel(page)
    return { page, agent }
  }

  // ---- invariants ----------------------------------------------------------------------------
  /** Nothing stays busy forever: every spinner or skeleton settles, or a clear state replaces it. */
  async expectSettled(page = this.page) {
    const busy = () =>
      page
        .locator(BUSY)
        .evaluateAll((els) =>
          els.map((el) => {
            const part = el.closest(
              "[data-component], [role=log], section, main"
            )
            return `${el.tagName.toLowerCase()}${el.getAttribute("data-slot") ? `[${el.getAttribute("data-slot")}]` : ""} in ${part?.getAttribute("data-component") ?? part?.tagName.toLowerCase() ?? "?"}: ${(el.textContent ?? "").trim().slice(0, 40)}`
          })
        )
        .catch(() => [] as string[]) // mid-navigation: judged on the next check
    await expect
      .poll(busy, {
        message: `something stays busy (stuck spinner or skeleton) on ${page.url()}`,
        timeout: this.options.settleMs ?? 10_000,
      })
      .toEqual([])
  }
  /** Each action is decided at most once (no duplicate writes). */
  expectNoDuplicateWrites() {
    const approved = this.confirms
      .filter((c) => c.decision === "approve")
      .map((c) => c.ticket)
    expect(approved, "an action was approved more than once").toEqual([
      ...new Set(approved),
    ])
  }
  /** The dock is always there; an open task stays reachable (its scene or its capsule). */
  async expectReachable(page = this.page) {
    if (!(await page.locator("[data-flow-scroll]").count())) return // login, errors: no shell
    await expect(
      page.getByRole("region", { name: labels.dock }),
      "the dock is gone"
    ).toBeVisible()
    const task = page.locator("[data-task-state]")
    const scene = page.locator('[aria-labelledby="flow-results-title"]')
    if ((await task.count()) && !(await scene.count()))
      await expect(
        task.first(),
        "a task exists but cannot be reached"
      ).toBeVisible()
  }
  async expectNoForeignData(page = this.page) {
    for (const text of this.options.foreign ?? [])
      await expect(
        page.locator("main"),
        `foreign data shown: ${text}`
      ).not.toContainText(text)
  }
  /** All continuous invariants for every open page. */
  async check() {
    for (const page of this.pages) {
      if (page.isClosed()) continue
      // The browser's offline error page (a navigation while down): judged once back online.
      if (page.url().startsWith("chrome-error://")) continue
      // A click may still be navigating: judge the page it lands on, not the one it leaves.
      await page
        .waitForLoadState("domcontentloaded", { timeout: 10_000 })
        .catch(() => {})
      await this.expectSettled(page)
      if (await page.locator("[data-flow-scroll]").count())
        await expectSingleScroller(page).catch(async (error: unknown) => {
          if (!String(error).includes("Execution context was destroyed"))
            throw error
          await page.waitForLoadState("domcontentloaded").catch(() => {})
          await expectSingleScroller(page)
        })
      await this.expectReachable(page)
      await this.expectNoForeignData(page)
    }
    this.expectNoDuplicateWrites()
    for (const problems of this.extraProblems)
      expect(problems, "problems in another tab").toEqual([]) // the list names each one
  }

  /**
   * A long session: weighted random steps until `steps` ran or `budgetMs` is spent, invariants
   * after each. On failure the seed and the trail are in the error and the report.
   */
  async run(
    steps: Step[],
    {
      count = Number(process.env.PERSONA_STEPS) || 40,
      budgetMs = Number(process.env.PERSONA_BUDGET_MS) || 90_000,
    } = {}
  ) {
    const total = steps.reduce((sum, s) => sum + s.weight, 0)
    const until = Date.now() + budgetMs
    for (let i = 0; i < count && Date.now() < until; i++) {
      let roll = this.random() * total
      const step = steps.find((s) => (roll -= s.weight) < 0) ?? steps.at(-1)!
      this.log(`${i}: ${step.name}`)
      if (process.env.PERSONA_VERBOSE)
        console.log(`[persona] step ${i}: ${step.name}`)
      try {
        await step.run()
        await this.check()
      } catch (error) {
        throw new Error(
          `Persona broke at step ${i} (${step.name}). Replay: PERSONA_SEED=${this.seed}\n` +
            `Trail:\n${this.trail.join("\n")}\n\n${String(error)}`,
          { cause: error }
        )
      }
    }
  }
}

/** The guarded Flow test with a seeded persona on a signed-in page (owner of w1 + w2). */
export const test = flowTest.extend<{ persona: Persona }>({
  persona: async ({ page, api }, use, info) => {
    const seed = seedOf()
    // The session's own budget plus room for setup and the final checks.
    // A step plus its checks on every tab can take ~30 s (injected failures retry before settling).
    info.setTimeout((Number(process.env.PERSONA_BUDGET_MS) || 90_000) + 120_000)
    info.annotations.push({ type: "seed", description: String(seed) })
    console.log(
      `[persona] ${info.title} · ${info.project.name} · PERSONA_SEED=${seed}`
    )
    void api
    const net = { down: false }
    const agent = await honestAgent(page, net)
    await use(new Persona(page, agent, seed, { foreign: [] }, net))
  },
})

export { expect }
