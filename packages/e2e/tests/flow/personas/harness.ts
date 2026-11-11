import type { BrowserContext, Locator, Page, Request } from "@playwright/test"

import {
  expect,
  expectSingleScroller,
  markInjected,
  watchProblems,
} from "../../../src/test"
import type { FakeApi } from "../../office/fake-api"
import { test as flowTest } from "../storefront-api"
import { s as shell } from "../shell"
import { fakeAgent, v, type Agent } from "../voice"

/**
 * SIMULATED users for Flow (mock API + mock voice behind Flow's real proxy). A persona is a seeded
 * random walk over realistic actions; after every step the invariants below are checked. A failure
 * prints the seed and the step trail, so `PERSONA_SEED=<seed>` replays it exactly.
 */

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

/** Every approval decision the browser sent (from the mock's view): one per ticket at most. */
export function confirmLog(page: Page) {
  const seen: { ticket: string; decision: string }[] = []
  page.on("request", (request: Request) => {
    if (new URL(request.url()).pathname !== "/api/voice/confirm") return
    try {
      const body = request.postDataJSON() as { ticket?: unknown; decision?: unknown }
      seen.push({ ticket: String(body.ticket), decision: String(body.decision) })
    } catch {
      seen.push({ ticket: "?", decision: "unreadable" })
    }
  })
  return seen
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

  constructor(
    readonly page: Page,
    readonly agent: Agent,
    readonly seed: number,
    readonly options: PersonaOptions = {}
  ) {
    this.random = rng(seed)
    this.confirms = confirmLog(page)
    this.pages = [page]
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
    if (!(await el.isEnabled().catch(() => false))) return false
    const clickCount = how === "single" ? 1 : how === "double" ? 2 : 3
    await el.click({ clickCount, timeout: 2_000 }).catch(() => {})
    return true
  }
  /** A random visible control in the frame: buttons and links, never ones leaving Flow. */
  async clickAnything(scope: Locator = this.page.locator("body"), avoid = /log out|uitloggen|ondoka/i) {
    const controls = scope.locator(
      'button:visible:not([disabled]), a[href]:visible:not([target="_blank"])'
    )
    const count = await controls.count()
    if (!count) return null
    const el = controls.nth(this.int(0, count - 1))
    const name = ((await el.getAttribute("aria-label")) ?? (await el.innerText().catch(() => "")))
      .trim()
      .slice(0, 40)
    if (avoid.test(name)) return null
    await el.click({ timeout: 2_000 }).catch(() => {})
    return name
  }
  /** Types into the dock and sends (Enter or the Send button). */
  async say(text: string, how: "enter" | "button" = this.chance(0.5) ? "enter" : "button") {
    const box = this.page.getByLabel(v.message)
    if (!(await box.isEditable().catch(() => false))) return false
    await box.fill(text)
    if (how === "enter") await box.press("Enter")
    else await this.click(this.page.getByRole("button", { name: v.send, exact: true }))
    return true
  }
  async startText() {
    return this.click(this.page.getByRole("button", { name: v.textInstead, exact: true }))
  }
  async backHome() {
    return this.click(this.page.getByRole("button", { name: shell.backHome }))
  }
  async keys(...keys: string[]) {
    for (const key of keys) await this.page.keyboard.press(key)
  }
  async tab(times = 1, back = false) {
    for (let i = 0; i < times; i++) await this.page.keyboard.press(back ? "Shift+Tab" : "Tab")
  }
  /** Back or Forward, only within Flow's own history (never back out to a blank tab). */
  async history(direction: "back" | "forward") {
    const can = await this.page
      .evaluate((back) => {
        const nav = (window as unknown as { navigation?: { canGoBack: boolean; canGoForward: boolean } })
          .navigation
        return back ? nav?.canGoBack : nav?.canGoForward
      }, direction === "back")
      .catch(() => false)
    if (!can) return
    await (direction === "back" ? this.page.goBack() : this.page.goForward()).catch(() => {})
  }
  async reload() {
    await this.page.reload().catch(() => {})
  }
  /** Network down for the page's requests (the mocked voice socket is not affected). */
  async offline(context: BrowserContext, on: boolean) {
    if (on) markInjected(this.page, /./)
    await context.setOffline(on)
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
    if (size) await this.page.setViewportSize({ width: size.height, height: size.width })
  }
  /** The person talks over the agent: an interruption, then their words. */
  bargeIn(words: string, id = this.int(1000, 9999)) {
    this.agent.emit({ type: "interruption", interruption_event: { event_id: id } })
    this.agent.emit({
      type: "user_transcript",
      user_transcription_event: { user_transcript: words, event_id: id + 1 },
    })
  }
  /** The same login in another tab, with its own guard, API and agent. */
  async secondTab(api: FakeApi) {
    const page = await this.page.context().newPage()
    this.extraProblems.push(watchProblems(page, {}, undefined))
    await page.route("**/v1/**", (route) => api.handle(route))
    const agent = await fakeAgent(page)
    this.pages.push(page)
    return { page, agent }
  }

  // ---- invariants ----------------------------------------------------------------------------
  /** Nothing stays busy forever: every spinner or skeleton settles, or a clear state replaces it. */
  async expectSettled(page = this.page) {
    await expect
      .poll(() => page.locator(BUSY).count(), {
        message: "something stays busy (stuck spinner or skeleton)",
        timeout: this.options.settleMs ?? 10_000,
      })
      .toBe(0)
  }
  /** Each action is decided at most once (no duplicate writes). */
  expectNoDuplicateWrites() {
    const approved = this.confirms.filter((c) => c.decision === "approve").map((c) => c.ticket)
    expect(approved, "an action was approved more than once").toEqual([...new Set(approved)])
  }
  /** The dock is always there; an open task stays reachable (its scene or its capsule). */
  async expectReachable(page = this.page) {
    if (!(await page.locator("[data-flow-scroll]").count())) return // login, errors: no shell
    await expect(page.getByRole("region", { name: shell.dock }), "the dock is gone").toBeVisible()
    const task = page.locator("[data-task-state]")
    const scene = page.locator('[aria-labelledby="flow-results-title"]')
    if ((await task.count()) && !(await scene.count()))
      await expect(task.first(), "a task exists but cannot be reached").toBeVisible()
  }
  async expectNoForeignData(page = this.page) {
    for (const text of this.options.foreign ?? [])
      await expect(page.locator("main"), `foreign data shown: ${text}`).not.toContainText(text)
  }
  /** All continuous invariants for every open page. */
  async check() {
    for (const page of this.pages) {
      if (page.isClosed()) continue
      await this.expectSettled(page)
      if (await page.locator("[data-flow-scroll]").count()) await expectSingleScroller(page)
      await this.expectReachable(page)
      await this.expectNoForeignData(page)
    }
    this.expectNoDuplicateWrites()
    for (const problems of this.extraProblems)
      expect(problems, "problems in another tab").toEqual([])
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
    info.setTimeout((Number(process.env.PERSONA_BUDGET_MS) || 90_000) + 60_000)
    info.annotations.push({ type: "seed", description: String(seed) })
    console.log(`[persona] ${info.title} · ${info.project.name} · PERSONA_SEED=${seed}`)
    void api
    // A fast, honest agent: answers each typed line a moment later, with its own event id.
    let id = 0
    const agent = await fakeAgent(page, (text, a) =>
      setTimeout(
        () =>
          a.emit({
            type: "agent_response",
            agent_response_event: { agent_response: `Noted: ${text.slice(0, 60)}`, event_id: ++id },
          }),
        80
      )
    )
    await use(new Persona(page, agent, seed, { foreign: [] }))
  },
})

export { expect }
