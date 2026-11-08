import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { expect, test, type Page, type Request } from "@playwright/test"

import { grade, isNarration } from "./grade.mjs"

type Scenario = {
  id: string
  module: string
  kind: string
  as?: "afm" | "ruaha"
  turns: string[]
  expect: { outcome: string; ops?: string[]; mention?: string; noTool?: boolean }
  gap?: string
  regression?: string
  rapid?: boolean // send the next line ~1 s after the previous one, mid-turn
  holdReview?: boolean // keep the review card open across turns (still never approved), cancel at the end
  attach?: string[] // files relative to this folder, attached to the first line
  setup?: { product?: string } // created through the API before the conversation (local stack only)
}
type Turn = { user: string; finished: string; ms: number }
type ToolCall = { command?: string; operationId?: string; query?: string; http: number; status?: string }

const read = (path: string) => JSON.parse(readFileSync(new URL(path, import.meta.url), "utf8"))
const bank = process.env.VOICE_PROBE_BANK ?? "scenarios" // or "adversarial"
const scenarios: Scenario[] = read(`./${bank}.json`)
const catalog = read("../../../apps/office/lib/voice/catalog.generated.json")
const results = new URL(`./out/${bank}/results/`, import.meta.url)

const only = process.env.VOICE_PROBE_ONLY?.split(",").filter(Boolean)
const module = process.env.VOICE_PROBE_MODULE
const staging = process.env.VOICE_PROBE_URL
const selected = scenarios.filter(
  (s) => (!only || only.includes(s.id)) && (!module || s.module === module)
)

const TURN_LIMIT = 150_000 // a turn that has not settled by then is broken, not slow
const SETTLE = 3_000 // transcript unchanged this long, nothing busy → the turn is over
const NARRATION_GRACE = 25_000 // after a "let me check…" line, how long a real answer may still take
// Only these batch commands are allowed; there is no approve among them, and any new one is blocked.
const BATCH_SAFE = new Set(["prepare", "restore", "status", "list", "pause", "cancel"])

async function login(page: Page, as: "afm" | "ruaha") {
  if (staging) {
    const email = process.env.VOICE_PROBE_EMAIL
    const password = process.env.VOICE_PROBE_PASSWORD
    if (!email || !password) throw new Error("VOICE_PROBE_URL needs VOICE_PROBE_EMAIL and VOICE_PROBE_PASSWORD")
    await page.goto(`${staging}/login`)
    await page.getByLabel("Email").fill(email)
    await page.getByLabel("Password").fill(password)
    await page.getByRole("button", { name: "Log in" }).click()
    await expect(page).not.toHaveURL(/\/login/)
    return staging
  }
  // Local full stack (STACK.md): seeded logins, never real credentials.
  const { cred, loginOffice, urls } = await import("../src/stack")
  await loginOffice(page, as === "ruaha" ? cred("office (RUAHA)", "admin") : cred("office (AFM)", "owner"))
  return urls.office
}

/** Blocks every approve/submit before it leaves the browser and counts the attempt. */
async function guardWrites(page: Page, attempts: string[]) {
  const block = (pattern: string, safe: (body: Record<string, unknown>) => boolean) =>
    page.route(pattern, (route) => {
      const req = route.request()
      if (req.method() !== "POST") return route.continue()
      let body: Record<string, unknown> | null
      try {
        body = req.postDataJSON()
      } catch {
        body = null
      }
      if (body && safe(body)) return route.continue()
      attempts.push(`${pattern} ${JSON.stringify(body)?.slice(0, 120)}`)
      return route.abort("blockedbyclient")
    })
  await block("**/api/voice/confirm", (b) => b.decision === "cancel")
  await block("**/api/voice/batch", (b) => BATCH_SAFE.has(String(b.command)))
  await block("**/api/voice/secure", (b) => b.command === "describe")
}

function recordTools(page: Page, calls: ToolCall[], inflight: Set<Request>) {
  page.on("request", (req) => {
    if (req.url().includes("/api/voice/tools")) inflight.add(req)
  })
  page.on("requestfailed", (req) => inflight.delete(req))
  page.on("response", async (res) => {
    const req = res.request()
    if (!req.url().includes("/api/voice/tools")) return
    let body: Record<string, unknown>
    try {
      body = req.postDataJSON() ?? {}
    } catch {
      body = {} // multipart (files): the probe sends none
    }
    let status: string | undefined
    try {
      status = (await res.json())?.status
    } catch {
      status = undefined
    }
    calls.push({
      command: body.command as string,
      operationId: body.operationId as string,
      query: body.query as string,
      http: res.status(),
      status,
    })
    inflight.delete(req)
  })
}

async function transcript(page: Page) {
  return page.locator('[role="log"] [data-role]').evaluateAll((rows) =>
    rows.map((row) => ({
      role: row.getAttribute("data-role") ?? "",
      // The sr-only "Genesy: " prefix is for screen readers; drop it.
      text: (row.textContent ?? "").replace(/^(You|Genesy|Action):\s*/, "").trim(),
    }))
  )
}

/** Waits until the turn is really over: not just quiet, but answered (or provably stuck). */
type ProbeState = { reviews: number; thinking: Set<string>; hold: boolean; held: boolean }

async function cancelReview(page: Page) {
  const approve = page.getByRole("button", { name: "Approve action" })
  await page.getByRole("button", { name: "Cancel", exact: true }).first().click()
  await expect(approve).toBeHidden({ timeout: 15_000 })
}

async function finishTurn(page: Page, before: number, inflight: Set<Request>, state: ProbeState) {
  const start = Date.now()
  let lastChange = Date.now()
  let lastKey = ""
  let narratedAt = 0
  while (Date.now() - start < TURN_LIMIT) {
    await page.waitForTimeout(500)
    const thinkingText = await page.locator("[data-voice-thinking]").allInnerTexts().catch(() => [])
    for (const block of thinkingText) for (const line of block.split("\n")) if (line.trim()) state.thinking.add(line.trim())
    const approve = page.getByRole("button", { name: "Approve action" })
    if (await approve.isVisible().catch(() => false)) {
      if (!state.held) state.reviews++
      if (state.hold) {
        // Leave it open so later lines can try to talk the agent into approving; it counts as the turn's answer.
        state.held = true
        if (Date.now() - lastChange > SETTLE && inflight.size === 0) return "answered"
      } else {
        // Never approve: cancel the card the assistant proposed.
        await cancelReview(page)
        lastChange = Date.now()
        continue
      }
    } else state.held = false
    const activity = page.locator("[data-voice-activity]")
    // "Waiting for your approval" counts as busy in the UI, but the agent's turn is over.
    const busy =
      (await activity.getAttribute("aria-busy").catch(() => null)) === "true" &&
      (await activity.getAttribute("data-state").catch(() => null)) !== "awaiting_review"
    const thinking = (await page.locator("[data-voice-thinking]").count()) > 0
    const rows = await transcript(page)
    const key = JSON.stringify(rows)
    if (key !== lastKey) {
      lastKey = key
      lastChange = Date.now()
    }
    const fresh = rows.slice(before).filter((r) => r.role === "agent")
    const quiet = !busy && !thinking && inflight.size === 0 && Date.now() - lastChange > SETTLE
    if (!quiet || (!fresh.length && state.reviews === 0)) continue
    const last = fresh.at(-1)?.text ?? ""
    if (!isNarration(last)) return "answered"
    narratedAt ||= Date.now()
    if (Date.now() - narratedAt > NARRATION_GRACE) return "narration_only"
  }
  const rows = await transcript(page)
  const fresh = rows.slice(before).filter((r) => r.role === "agent")
  if (fresh.length && isNarration(fresh.at(-1)!.text)) return "narration_only"
  return fresh.length ? "timeout" : "no_reply"
}

for (const scenario of selected) {
  test(scenario.id, async ({ page }) => {
    test.setTimeout(scenario.turns.length * (TURN_LIMIT + 15_000) + 90_000)
    // VOICE_PROBE_RESUME=1: a scenario already graded in this bank is not paid for twice.
    test.skip(
      process.env.VOICE_PROBE_RESUME === "1" && existsSync(new URL(`${scenario.id}.json`, results)),
      "already graded"
    )
    test.skip(Boolean(staging) && scenario.as === "ruaha", "staging run uses one login (AFM)")
    // RUAHA needs to be on the stack's assistant allowlist (VOICE_WORKSPACES=AFM,RUAHA).
    test.skip(!staging && scenario.as === "ruaha" && process.env.VOICE_PROBE_RUAHA !== "1", "RUAHA not allowlisted")
    const attempts: string[] = []
    const calls: ToolCall[] = []
    const inflight = new Set<Request>()
    const state: ProbeState = { reviews: 0, thinking: new Set<string>(), hold: Boolean(scenario.holdReview), held: false }
    await guardWrites(page, attempts)
    recordTools(page, calls, inflight)

    const office = await login(page, scenario.as ?? "afm")
    if (scenario.setup?.product) {
      test.skip(Boolean(staging), "setup writes data: local stack only")
      const { apiAs, cred } = await import("../src/stack")
      const owner = await apiAs(page.request, scenario.as === "ruaha" ? cred("office (RUAHA)", "admin") : cred("office (AFM)", "owner"))
      await owner.post(owner.ws("/products"), { name: scenario.setup.product, price: "4.00" })
    }
    await page.goto(`${office}/voice`)
    await page.getByRole("button", { name: "Use text instead" }).click()
    const box = page.getByRole("textbox", { name: "Message the assistant" })
    await expect(box).toBeVisible({ timeout: 30_000 })

    const turns: Turn[] = []
    if (scenario.attach?.length)
      await page
        .locator('input[type="file"]')
        .first()
        .setInputFiles(scenario.attach.map((f) => new URL(f, import.meta.url).pathname))
    for (const [i, line] of scenario.turns.entries()) {
      const before = (await transcript(page)).length
      const started = Date.now()
      await box.fill(line)
      await box.press("Enter")
      if (scenario.rapid && i < scenario.turns.length - 1) {
        await page.waitForTimeout(1_000)
        turns.push({ user: line, finished: "answered", ms: Date.now() - started })
        continue
      }
      const finished = await finishTurn(page, before, inflight, state)
      turns.push({ user: line, finished, ms: Date.now() - started })
      if (finished !== "answered") break
    }

    if (state.held && (await page.getByRole("button", { name: "Approve action" }).isVisible().catch(() => false)))
      await cancelReview(page)
    const messages = await transcript(page)
    const alerts = (await page.locator('main [role="alert"]').allInnerTexts().catch(() => [])).filter(Boolean)
    const record = {
      turns,
      messages,
      toolCalls: calls,
      reviews: state.reviews,
      handoffs: calls.filter((c) => c.status === "handoff").length,
      approveAttempts: attempts.length,
      alerts,
      thinking: [...state.thinking],
    }
    const verdict = grade(scenario, record, catalog)
    mkdirSync(results, { recursive: true })
    writeFileSync(
      new URL(`${scenario.id}.json`, results),
      JSON.stringify({ scenario, record, verdict, attempts }, null, 2)
    )
    await page.getByRole("button", { name: "End conversation" }).click({ timeout: 5_000 }).catch(() => {})
    test.info().annotations.push({ type: verdict.class, description: verdict.why })
    // The probe never writes: any approve/submit attempt fails the run, even though it was blocked.
    expect(attempts, "approve/submit attempted").toEqual([])
  })
}
