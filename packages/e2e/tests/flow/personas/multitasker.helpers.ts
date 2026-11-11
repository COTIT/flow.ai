// Persona 5 helpers (the multitasker): tabs on one login, a cross-tab ledger of reviews and
// decisions, and actions that speak each tab's language. SIMULATED API and voice (see harness.ts).
import type { Page, Route } from "@playwright/test"

import voiceEn from "../../../../../packages/i18n/messages/voice/en.json" with { type: "json" }
import voiceNl from "../../../../../packages/i18n/messages/voice/nl.json" with { type: "json" }
import voiceSw from "../../../../../packages/i18n/messages/voice/sw.json" with { type: "json" }
import shellEn from "../../../../../apps/flow/messages/en/shell.json" with { type: "json" }
import shellNl from "../../../../../apps/flow/messages/nl/shell.json" with { type: "json" }
import shellSw from "../../../../../apps/flow/messages/sw/shell.json" with { type: "json" }
import commonEn from "../../../../../packages/i18n/messages/en.json" with { type: "json" }
import commonNl from "../../../../../packages/i18n/messages/nl.json" with { type: "json" }
import commonSw from "../../../../../packages/i18n/messages/sw.json" with { type: "json" }
import { expect, markInjected, watchProblems } from "../../../src/test"
import type { FakeApi } from "../../office/fake-api"
import { ACTION, stockReview } from "../task"
import { revealHeader } from "../shell"
import { fakeAgent, type Agent } from "../voice"
import type { Persona } from "./harness"

export type Locale = "en" | "nl" | "sw"
const pack = (voice: typeof voiceEn, shell: typeof shellEn, common: typeof commonEn) => ({
  textInstead: voice.voice.textInstead,
  message: voice.voice.message,
  send: voice.voice.send,
  approve: voice.voice.approve,
  addFiles: voice.voice.addFiles,
  cancel: voice.voice.cancel,
  backHome: shell.shell.backHome,
  dock: shell.shell.dock,
  language: common.common.language,
  workspace: common.common.workspace,
  logout: common.common.logout,
})
export const L: Record<Locale, ReturnType<typeof pack>> = {
  en: pack(voiceEn, shellEn, commonEn),
  nl: pack(voiceNl as typeof voiceEn, shellNl as typeof shellEn, commonNl as typeof commonEn),
  sw: pack(voiceSw as typeof voiceEn, shellSw as typeof shellEn, commonSw as typeof commonEn),
}
export const localeOf = async (page: Page): Promise<Locale> => {
  const lang = await page.evaluate(() => document.documentElement.lang).catch(() => "en")
  return (lang in L ? lang : "en") as Locale
}

/** One review the mock Office handed out, and what was decided on it (from any tab). */
type Review = { ticket: string; actionId: string; ws: string; tab: number; session: number; json: object; decided: string | null }
/** Everything the tabs proposed and decided, across tabs (the mock's view, like Office's). */
export class Ledger {
  seq = 0
  reviews = new Map<string, Review>()
  /** Every decision request the browser sent, from every tab (refused ones too). */
  decisions: { tab: number; ticket: string; decision: string }[] = []
  /** Markers typed or shown per workspace: after a switch, the other workspace's are foreign. */
  markers: Record<string, string[]> = {}
  /** Milliseconds the next confirm waits (a slow network while the person moves on). */
  confirmDelay = 0
  restores: string[] = []

  mark(ws: string, marker: string) {
    ;(this.markers[ws] ??= []).push(marker)
    return marker
  }
  /** Same action approved twice, by any tabs: Office would refuse the second; the UI let it go. */
  approvedTwice() {
    const seen = this.decisions.filter((d) => d.decision === "approve").map((d) => d.ticket)
    return seen.filter((t, i) => seen.indexOf(t) !== i)
  }
}

const json = (route: Route) => (route.request().postDataJSON() ?? {}) as Record<string, unknown>

/**
 * Tools + confirm + restore for one tab: each stock proposal gets its own ticket and action id
 * and names its workspace in the record label (so a leak shows as foreign text).
 */
export async function multitaskRoutes(page: Page, ledger: Ledger, tab: number, session = tab) {
  await page.route("**/api/voice/tools", async (route) => {
    const b = json(route)
    const ws = String(b.workspaceId ?? "w?")
    // Office's task list for this voice session (a duplicated tab shares its source's session).
    if (b.command === "status" && !b.batchId) {
      ledger.restores.push(`${tab}:status`)
      const data = [...ledger.reviews.values()]
        .filter((r) => r.session === session && r.ws === ws)
        .map((r) =>
          r.decided
            ? { status: r.decided === "approve" ? "completed" : "cancelled", data: { actionId: r.actionId, summary: "Set stock levels" } }
            : { status: "awaiting_review", data: { actionId: r.actionId, recoverable: true, summary: "Set stock levels" } }
        )
      return route.fulfill({ json: { status: "completed", data } })
    }
    if (b.command === "restore") {
      const id = String(b.actionId)
      ledger.restores.push(`${tab}:${id}`)
      const r = ledger.reviews.get(id)
      if (r && !r.decided) return route.fulfill({ json: r.json })
      return route.fulfill({ json: { status: "completed", actionId: id, data: { message: "ok" } } })
    }
    if (b.command !== "execute" || b.operationId !== "stock_set_levels") return route.fallback()
    const n = ++ledger.seq
    const args = b.arguments as { body: { warehouses: { stock_lvl: number }[] } }
    const total = args.body.warehouses[0]!.stock_lvl
    const ticket = `t${tab}-${n}`
    const actionId = ACTION(9000 + n)
    const base = stockReview(total, ticket)
    const marker = ledger.mark(ws, `${ws.toUpperCase()}R${n}x`)
    const reply = {
      ...base,
      data: { actionId },
      review: {
        ...base.review,
        summary: `Set stock levels ${marker}`,
        details: { ...base.review.details, record: { type: "Product", label: `Raw Honey 500g ${marker}` } },
      },
    }
    ledger.reviews.set(actionId, { ticket, actionId, ws, tab, session, json: reply, decided: null })
    return route.fulfill({ json: reply })
  })
  await page.route("**/api/voice/confirm", async (route) => {
    const b = json(route)
    const ticket = String(b.ticket)
    const decision = String(b.decision)
    ledger.decisions.push({ tab, ticket, decision })
    const r = [...ledger.reviews.values()].find((x) => x.ticket === ticket)
    const delay = ledger.confirmDelay
    if (delay) await new Promise((res) => setTimeout(res, delay))
    if (r?.decided)
      // Office: a ticket is used once. The second decision is refused in plain words.
      return route
        .fulfill({ json: { status: "failed", message: "This action was already decided." } })
        .catch(() => {})
    if (r) r.decided = decision
    return route
      .fulfill({
        json:
          decision === "approve"
            ? { status: "completed", actionId: r?.actionId ?? ACTION(0), data: { message: "ok" } }
            : { status: "cancelled", actionId: r?.actionId ?? ACTION(0) },
      })
      .catch(() => {}) // the tab may be gone (closed mid-request)
  })
}

/** An answering fake agent: replies to each typed line with its own event id. */
export async function answeringAgent(page: Page, ids = { n: 1000 }) {
  return fakeAgent(page, (text, a) =>
    setTimeout(
      () =>
        a.emit({
          type: "agent_response",
          agent_response_event: { agent_response: `Noted: ${text.slice(0, 60)}`, event_id: ++ids.n },
        }),
      80
    )
  )
}

export type Tab = { page: Page; agent: Agent; n: number }

/**
 * Another tab on the same login (like the harness's secondTab), with an answering agent and this
 * persona's routes. `duplicateOf` copies that tab's sessionStorage first, as the browser's
 * "Duplicate tab" (and a same-origin window.open) does.
 */
export async function openTab(
  persona: Persona,
  api: FakeApi,
  ledger: Ledger,
  n: number,
  { url = "/", duplicateOf, session = n }: { url?: string; duplicateOf?: Page; session?: number } = {}
): Promise<Tab> {
  const page = await persona.page.context().newPage()
  persona.extraProblems.push(watchProblems(page, {}, undefined))
  if (duplicateOf) {
    const copied = await duplicateOf
      .evaluate(() => JSON.stringify(Object.entries(sessionStorage)))
      .catch(() => "[]")
    await page.addInitScript((entries) => {
      if (sessionStorage.getItem("__dup")) return
      for (const [k, v] of JSON.parse(entries) as [string, string][]) sessionStorage.setItem(k, v)
      sessionStorage.setItem("__dup", "1")
    }, copied)
  }
  await page.route("**/v1/**", (route) => api.handle(route))
  const agent = await answeringAgent(page)
  await multitaskRoutes(page, ledger, n, session)
  persona.pages.push(page)
  await page.goto(url).catch(() => {})
  return { page, agent, n }
}

// ---- actions in the tab's own language ------------------------------------------------------
async function clickIf(page: Page, name: string, exact = true) {
  const el = page.getByRole("button", { name, exact }).first()
  if (!(await el.isVisible().catch(() => false)) || !(await el.isEnabled().catch(() => false))) return false
  await el.click({ timeout: 2_000 }).catch(() => {})
  return true
}
export async function startText(page: Page) {
  return clickIf(page, L[await localeOf(page)].textInstead)
}
export async function say(page: Page, text: string, enter = true) {
  const t = L[await localeOf(page)]
  const box = page.getByLabel(t.message)
  if (!(await box.isEditable().catch(() => false))) return false
  await box.fill(text)
  if (enter) await box.press("Enter")
  else await clickIf(page, t.send)
  return true
}
/** Connected: the dock's attach button exists only then. */
export const live = async (page: Page) =>
  page.getByRole("button", { name: L[await localeOf(page)].addFiles }).isVisible().catch(() => false)
export async function approve(page: Page, how: 1 | 2 = 1) {
  const el = page.getByRole("button", { name: L[await localeOf(page)].approve }).first()
  if (!(await el.isVisible().catch(() => false)) || !(await el.isEnabled().catch(() => false))) return false
  await el.click({ clickCount: how, timeout: 2_000 }).catch(() => {})
  return true
}
export async function cancel(page: Page) {
  return clickIf(page, L[await localeOf(page)].cancel)
}
export async function backHome(page: Page) {
  return clickIf(page, L[await localeOf(page)].backHome, false)
}
/** The workspace this browser's login points at (shared by every tab). */
export const savedWorkspace = (page: Page) =>
  page
    .evaluate(() => (JSON.parse(localStorage.getItem("genesy.session") ?? "{}") as { state?: { workspaceId?: string } }).state?.workspaceId ?? null)
    .catch(() => null)
/** Picks the other workspace in this tab's header picker, as a person would. */
export async function switchWorkspace(page: Page, to: string) {
  await showHeader(page)
  const t = L[await localeOf(page)]
  const picker = page.getByRole("combobox", { name: t.workspace })
  if (!(await picker.isVisible().catch(() => false))) return false
  await picker.click({ timeout: 2_000 }).catch(() => {})
  const option = page.getByRole("option", { name: to, exact: true })
  const shown = await option.waitFor({ state: "visible", timeout: 2_000 }).then(() => true, () => false)
  if (!shown) {
    await page.keyboard.press("Escape")
    return false
  }
  await option.click({ timeout: 2_000 }).catch(() => {})
  return true
}
export async function setLanguage(page: Page, to: Locale) {
  await showHeader(page)
  const t = L[await localeOf(page)]
  const select = page.getByRole("combobox", { name: t.language })
  if (!(await select.isVisible().catch(() => false))) return false
  await select.selectOption(to, { timeout: 2_000 }).catch(() => {})
  return true
}
export async function logOut(page: Page) {
  await showHeader(page)
  const label = L[await localeOf(page)].logout
  const done = await clickIf(page, label, false)
  // While a conversation or task is live, Log out asks first (P10-3): confirm.
  const ask = page.getByRole("alertdialog")
  if (done && (await ask.waitFor({ state: "visible", timeout: 1_000 }).then(() => true, () => false)))
    await ask.getByRole("button", { name: label }).click({ timeout: 2_000 }).catch(() => {})
  return done
}
/** The header floats hidden until asked for (D2): reveal it before using its controls. */
async function showHeader(page: Page) {
  await revealHeader(page).catch(() => {})
}

// ---- invariants of this persona -------------------------------------------------------------
/** A tab speaks one language: its <html lang>, its language picker and its dock agree. */
export async function expectOneLanguage(page: Page) {
  if (page.isClosed() || !(await page.locator("[data-flow-scroll]").count())) return
  const lang = await localeOf(page)
  const t = L[lang]
  await expect(page.getByRole("region", { name: t.dock }), `dock not in ${lang} (mixed languages)`).toBeVisible()
  const picker = page.getByRole("combobox", { name: t.language })
  if (await picker.count()) await expect(picker, "language picker disagrees with the page").toHaveValue(lang)
}
/** Allows failures a closed or reloaded tab causes on purpose. */
export const allowAborts = (page: Page) => markInjected(page, /\/api\/voice\//)
