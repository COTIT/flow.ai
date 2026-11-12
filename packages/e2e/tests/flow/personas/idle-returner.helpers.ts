// Persona 8 helpers (the idle returner): a server whose login can expire, a page that can be hidden
// or put to sleep, and the way back in. SIMULATED API and voice (see harness.ts).
import type { Page } from "@playwright/test"

import { markInjected } from "../../../src/test"
import type { FakeApi } from "../../office/fake-api"
import { v } from "../voice"

/**
 * The server's view of the login: once `expired`, every /v1 call (but /login) and every voice route
 * answers 401, as the API and Office do for a token past its expiry. A new login renews it.
 */
export class Server {
  expired = false
  /** 401s this page saw (they are expected once the login expired). */
  refused: string[] = []
  constructor(readonly api: FakeApi) {
    api.on("POST", "/login", () => {
      this.expired = false
      return {
        data: {
          access_token: "e2e-token",
          token_type: "Bearer",
          expires_at: "2099-01-01T00:00:00Z",
          workspace_ids: ["w1", "w2"],
        },
      }
    })
  }
  /** Registered after the fixture's routes, so it answers first. */
  async attach(page: Page) {
    // Readiness carries no login (Flow asks it without a token), so an expired one never refuses it.
    const refuse = (path: string) =>
      this.expired &&
      !path.endsWith("/v1/login") &&
      !path.endsWith("/v1/logout") &&
      !path.endsWith("/api/voice/readiness")
    for (const pattern of ["**/v1/**", "**/api/voice/**"])
      await page.route(pattern, (route) => {
        const path = new URL(route.request().url()).pathname
        if (!refuse(path)) return route.fallback()
        this.refused.push(`${route.request().method()} ${path}`)
        return route.fulfill({ status: 401, json: { message: "Unauthenticated." } })
      })
  }
  /** The login expires on the server (time passed); the page will learn on its next call. */
  expire(page: Page) {
    this.expired = true
    markInjected(page, /\/v1\/|\/api\/voice\//)
  }
}

/** The tab goes to the background (another app, a locked phone) and, later, comes back. */
export async function hide(page: Page, hidden = true) {
  await page.evaluate((h) => {
    Object.defineProperty(document, "visibilityState", { value: h ? "hidden" : "visible", configurable: true })
    Object.defineProperty(document, "hidden", { value: h, configurable: true })
    document.dispatchEvent(new Event("visibilitychange"))
    window.dispatchEvent(new Event(h ? "blur" : "focus"))
  }, hidden)
}

/** The laptop sleeps: the page's timers freeze, then wake `ms` later all at once. */
export async function sleep(page: Page, ms: number, { hidden = true } = {}) {
  // The page's own clock (installed earlier) may run ahead of the test's.
  await page.clock.pauseAt((await page.evaluate(() => Date.now())) + 1_000)
  if (hidden) await hide(page, true)
  await page.clock.fastForward(ms)
  if (hidden) await hide(page, false)
  await page.clock.resume()
}

export async function logInAgain(page: Page) {
  await page.getByLabel("Email").fill("e2e@genesy.test")
  await page.getByLabel("Password").fill("e2e-password-only")
  await page.getByRole("button", { name: "Log in" }).click()
}

export const photo = {
  name: "shelf.png",
  mimeType: "image/png",
  // 1×1 transparent PNG.
  buffer: Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=",
    "base64"
  ),
}
export const tray = (page: Page) => page.getByRole("list", { name: v.attachments })

const steps = (done: number, total = 21) =>
  Array.from({ length: total }, (_, i) => ({
    status: i < done ? "completed" : "not_started",
    data: { actionId: `a-${i}`, operation: "products_update", summary: `Price ${i}` },
  }))
const progress = (state: string, done: number) => ({
  status: "partial",
  data: { kind: "batch", batchId: "b-1", state, nextIndex: done, recoverable: false, steps: steps(done) },
})
/**
 * Office's batch shapes (as task-supplier.spec): prepare → review of 21 price updates; approve
 * starts a run that stays "running" at step 4 until paused. Returns the batch commands and confirms.
 */
export async function batchRoutes(page: Page) {
  const commands: string[] = []
  const confirms: string[] = []
  let state = "running"
  // The run holds the confirm open until it is paused (as Office's run does), polled meanwhile.
  let release: () => void = () => {}
  const stopped = new Promise<void>((resolve) => (release = resolve))
  await page.route("**/api/voice/batch", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>
    commands.push(String(body.command))
    if (body.command === "prepare")
      return route.fulfill({
        json: {
          status: "requires_confirmation",
          data: { batchId: "b-1" },
          review: {
            ticket: "batch-21",
            kind: "batch",
            batchId: "b-1",
            summary: "Update 21 supplier purchase prices",
            confirmationCode: "246810",
            confirmationPhrase: "Confirm batch 246810",
            details: { notIncluded: [] },
            steps: steps(0).map((s) => ({ actionId: s.data.actionId, operationId: "products_update", summary: s.data.summary, details: {} })),
            expiresAt: new Date(Date.now() + 10 * 60_000).toISOString(),
          },
        },
      })
    if (body.command === "status") return route.fulfill({ json: progress(state, state === "paused" ? 8 : 4) })
    if (body.command === "pause") {
      state = "paused"
      release()
      return route.fulfill({ json: progress("paused", 8) })
    }
    return route.fallback()
  })
  await page.route("**/api/voice/confirm", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>
    if (String(body.ticket) !== "batch-21") return route.fallback()
    confirms.push(String(body.decision))
    await stopped
    return route.fulfill({ json: progress("paused", 8) }).catch(() => {})
  })
  return { commands, confirms, release }
}
