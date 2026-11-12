// Persona 6, the flaky-network commuter: the train goes through tunnels. Offline and online at
// random moments, slow 3G, requests dropped mid-flight, answers late or out of order, reloads
// while waiting, the voice socket lost mid-turn. SIMULATED API and voice (see harness.ts).
import type { CDPSession, Page } from "@playwright/test"

import flowEn from "../../../../../apps/flow/messages/en.json" with { type: "json" }
import { markInjected } from "../../../src/test"
import { sc } from "../scenes-api"
import { s as shell } from "../shell"
import { readBack, scene, stockRoutes } from "../task"
import { startText, v, typeInstead } from "../voice"
import { expect, test, type Persona } from "./harness"

const SCENES = ["pulse", "stock", "noticed", "orders", "sales", "routines"] as const
/** Raw codes a person must never read (HTTP statuses, error names, ids). A status counts only as
 * its own token after such a word, never inside money or a number ("€480.00", "500g", "4500"). */
const RAW = /\b(?:HTTP|status|error|code)[\s:]*[45]\d\d(?!\w|[.,]\d)|\b(?:ERR_[A-Z_]+|TypeError|SyntaxError|Failed to fetch|NetworkError|Load failed|undefined|NaN)\b|\bnull\b(?! ?[a-z])|is not valid JSON|Unexpected token/i

/** Chrome DevTools "Slow 3G" (latency ms, bytes/s); off restores the line. */
let throttled = false
let calmSince = 0
/** Code chunks lost on a bad line: while slow 3G is on, and for 3 s after (a real tunnel, not a bug). */
const tunnelledChunks = {
  test: (url: string) =>
    /\/_next\/static\/chunks\//.test(url) && (throttled || Date.now() - calmSince < 3_000),
}
async function throttle(cdp: CDPSession, on: boolean) {
  if (throttled && !on) calmSince = Date.now()
  throttled = on
  await cdp.send("Network.emulateNetworkConditions", {
    offline: false,
    latency: on ? 2000 : 0,
    downloadThroughput: on ? 50_000 : -1,
    uploadThroughput: on ? 50_000 : -1,
  })
}

/**
 * A short tunnel: HTTP is down (mocks included) but the voice socket is still "open", as a
 * browser keeps a dead WebSocket open for a while. persona.offline also drops the socket.
 */
async function shortTunnel(page: Page) {
  let down = false
  await page.route("**/*", (route) => (down ? route.abort("internetdisconnected") : route.fallback()))
  return async (on: boolean) => {
    if (on) markInjected(page, /./)
    down = on
    await page.context().setOffline(on)
  }
}

/** The request leaves, the answer never comes back (the train enters a tunnel). */
async function dropOnce(page: Page, path: RegExp) {
  markInjected(page, path)
  let left = 1
  await page.route(path, (route) =>
    left-- > 0 ? route.abort("connectionreset") : route.fallback()
  )
}

/** Answers arrive late: each matching request waits `ms` before it goes on. */
async function delay(page: Page, path: RegExp, ms: number, times = 1) {
  let left = times
  await page.route(path, async (route) => {
    if (left-- > 0) await new Promise((r) => setTimeout(r, ms))
    await route.fallback().catch(() => {})
  })
}

/** Already reported (P6-1); the walk looks past them so it can find the next break. */
const KNOWN = [/Failed to fetch/g, /Unexpected token '<', .*? is not valid JSON/g]
async function rawOnScreen(page: Page, skipKnown = false) {
  let text = await page.locator("body").innerText().catch(() => "")
  if (skipKnown) for (const re of KNOWN) text = text.replace(re, " ")
  return text.match(RAW)?.[0] ?? null
}

/** Shared tunnel steps for the walk. */
function tunnelSteps(persona: Persona, page: Page, cdp: CDPSession) {
  let offline = false
  const context = page.context()
  let slow = false
  const live = () => page.getByRole("button", { name: v.addFiles }).isVisible()
  return [
    {
      name: "tunnel: offline for a moment",
      weight: 3,
      run: async () => {
        await persona.offline(context, true)
        offline = true
        await page.waitForTimeout(persona.int(200, 3000))
        if (persona.chance(0.7)) {
          await persona.offline(context, false)
          offline = false
        }
      },
    },
    {
      name: "out of the tunnel: online",
      weight: 2,
      run: async () => {
        if (!offline) return
        await persona.offline(context, false)
        offline = false
      },
    },
    {
      name: "slow 3G on/off",
      weight: 2,
      run: async () => {
        slow = !slow
        await throttle(cdp, slow)
      },
    },
    {
      name: "a voice route drops mid-flight",
      weight: 2,
      run: () => dropOnce(page, persona.pick([/\/api\/voice\/tools/, /\/api\/voice\/session/, /\/api\/voice\/confirm/])),
    },
    {
      name: "an API read answers 503",
      weight: 2,
      run: () => persona.failApi(persona.pick([/\/v1\/workspace\/w1\/orders/, /\/v1\/workspace\/w1\/attention/, /\/v1\/workspace\/w1\/products/]), 503, persona.int(1, 3)),
    },
    {
      name: "answers come late",
      weight: 2,
      run: () => delay(page, persona.pick([/\/api\/voice\/tools/, /\/v1\/workspace\/w1\//, /\/api\/voice\/confirm/]), persona.int(500, 4000), persona.int(1, 2)),
    },
    {
      name: "reload while waiting",
      weight: 2,
      run: async () => {
        if (offline) {
          await persona.offline(context, false)
          offline = false
        }
        await persona.reload()
      },
    },
    {
      name: "voice socket lost mid-turn",
      weight: 2,
      run: async () => {
        if (!(await live())) return
        await persona.say(persona.pick(["Sales today?", "Which orders wait?"]))
        persona.agent.close()
      },
    },
    {
      name: "open a scene",
      weight: 3,
      run: async () => {
        if (offline) return // a dead link offline is the browser's page, not Flow's
        await page.goto(`/scenes/${persona.pick(SCENES)}`).catch(() => {})
      },
    },
    {
      name: "back to home",
      weight: 2,
      // In a tunnel this leaves Flow for the browser's error page (P6-6, reported).
      run: async () => (offline ? undefined : persona.backHome()),
    },
    { name: "refresh the scene", weight: 2, run: () => persona.click(page.getByRole("button", { name: sc.refresh })) },
    { name: "try again", weight: 2, run: () => persona.click(page.getByRole("button", { name: /try again/i })) },
    { name: "start typing a conversation", weight: 2, run: () => persona.startText() },
    {
      name: "ask something",
      weight: 3,
      run: () => persona.say(persona.pick(["Sales today?", "Which orders wait?", "Stock below minimum", "Thanks"])),
    },
    {
      name: "a stock change to review",
      weight: 2,
      run: async () => {
        if (!(await live())) return
        persona.agent.call("stock_set_levels", {
          path: { product: "p-500" },
          body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: persona.pick([110, 120, 130]) }] },
        })
      },
    },
    {
      name: "approve (maybe twice in frustration)",
      weight: 3,
      run: () => persona.click(scene(page).getByRole("button", { name: v.approve }), persona.chance(0.3) ? "double" : "single"),
    },
    {
      name: "back online, settle",
      weight: 1,
      run: async () => {
        await persona.offline(context, false)
        offline = false
        await throttle(cdp, false)
        slow = false
      },
    },
  ]
}

test.describe("walk", () => {
  // P6-4 (reported): the SDK logs these when a tool answers after the socket closed.
  test.use({
    allow: {
      consolePrefix: [
        "Client tool execution failed with following error: WebSocket is already in CLOSING or CLOSED state.",
        "Unexpected error in client tool call handling: WebSocket is already in CLOSING or CLOSED state.",
      ],
    },
  })
/** Steps per walk: PERSONA_STEPS wins; CI runs a short walk (suite budget), local runs the full one. */
const STEPS = Number(process.env.PERSONA_STEPS) || (process.env.CI ? 12 : 40)

test("flaky network: tunnels, slow 3G, dropped and late answers, lost socket", async ({ persona, page, api }) => {
  const stock = readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
  api.on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => stock.body as object)
  await stockRoutes(page)
  const cdp = await page.context().newCDPSession(page)
  await page.emulateMedia({ colorScheme: persona.chance(0.5) ? "dark" : "light" })
  throttled = false
  calmSince = 0
  markInjected(page, tunnelledChunks) // offline already marks every request (persona.offline)
  await page.goto("/")
  const steps = tunnelSteps(persona, page, cdp).map((step) => ({
    ...step,
    run: async () => {
      await step.run()
      expect(await rawOnScreen(page, true), "raw code on screen").toBeNull()
    },
  }))
  await persona.run(steps, { count: STEPS })
  // Back on a good line, the app recovers by itself (or with one click).
  await persona.offline(page.context(), false)
  await throttle(cdp, false)
  await persona.check()
  // Back online the page is Flow again: its own screen or "This page couldn't load", with the
  // header and the dock; never the root crash card or a blank page.
  await expect(page.locator("[data-flow-scroll]"), "no Flow shell back online (blank or crashed)").toHaveCount(1)
  await expect(page.locator("[data-flow-header]")).toBeAttached()
  await expect(page.getByRole("region", { name: shell.dock })).toBeVisible()
  await expect(page.getByText(flowEn.flow.crashed)).toHaveCount(0)
})
})

// ---- targeted probes (each one a commuter moment) ---------------------------------------------


// ---- findings (red until fixed; ids reported to the coordinator) -----------------------------

/** The dock's alert, where the voice provider puts its errors. */
const alertText = async (page: Page) => (await page.getByRole("alert").allInnerTexts()).join(" | ")

test.describe("P6-1: a dropped or garbled voice request shows the browser's raw error", () => {
  // Fixed: regression guards.
  test("P6-1a: Use text instead while offline", async ({ persona, page }) => {
    await page.goto("/")
    await expect(page.getByRole("button", { name: typeInstead, exact: true })).toBeVisible()
    await persona.offline(page.context(), true)
    await persona.startText()
    await expect(page.getByRole("alert").first()).not.toBeEmpty()
    expect(await alertText(page)).not.toMatch(RAW)
  })
  test("P6-1b: a tool answer lost mid-turn", async ({ persona, page }) => {
    await page.goto("/")
    await startText(page)
    await dropOnce(page, /\/api\/voice\/tools/)
    persona.agent.call("products_list", {})
    await expect(page.getByRole("alert").first()).not.toBeEmpty()
    expect(await alertText(page)).not.toMatch(RAW)
  })
  test("P6-1c: the gateway answers an HTML 502", async ({ persona, page }) => {
    await page.goto("/")
    await expect(page.getByRole("button", { name: typeInstead, exact: true })).toBeVisible()
    markInjected(page, /\/api\/voice\//)
    let left = 1
    await page.route(/\/api\/voice\/session/, (route) =>
      left-- > 0
        ? route.fulfill({ status: 502, contentType: "text/html", body: "<html><body>Bad Gateway</body></html>" })
        : route.fallback()
    )
    await persona.startText()
    await expect(page.getByRole("alert").first()).not.toBeEmpty()
    expect(await alertText(page)).not.toMatch(RAW)
  })
})

// P6-2 (fixed): the regression test lives in tests/flow/voice-results.spec.ts (reads run in turn).

test.describe("P6-3: reloading while an approval is on its way loses the task", () => {
  for (const officeSays of ["running", "completed", "unknown"] as const)
    test(`P6-3: Office says ${officeSays}; the card or its state must come back`, async ({ persona, page, api }) => {
      const stock = readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
      api.on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => stock.body as object)
      const confirms = await stockRoutes(page)
      await page.route(/\/api\/voice\/tools/, (route) => {
        const b = route.request().postDataJSON() as { command?: string }
        if (b.command !== "status") return route.fallback()
        return route.fulfill({
          json: {
            status: "completed",
            data: [{ status: officeSays, data: { actionId: "0193f6b0-1c2d-7e3f-8a9b-000000000120", operation: "stock_set_levels", summary: "Set stock levels per warehouse" } }],
          },
        })
      })
      await page.goto("/")
      await startText(page)
      persona.agent.call("stock_set_levels", {
        path: { product: "p-500" },
        body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 120 }] },
      })
      await expect(scene(page)).toContainText("Confirm action 135790")
      // The approval leaves the page and is held 3 s; a reload then cancels the held request in
      // this browser, so "sent" counts it at the moment it left (Office would have it).
      const sent: string[] = []
      await page.route(/\/api\/voice\/confirm/, async (route) => {
        sent.push(route.request().url())
        await new Promise((r) => setTimeout(r, 3000))
        await route.fallback().catch(() => {})
      })
      markInjected(page, /\/api\/voice\/confirm/)
      await scene(page).getByRole("button", { name: v.approve }).click()
      // The approval is on the wire (Office will get it), the answer not back yet: the person reloads.
      await page.waitForTimeout(300)
      expect(confirms).toHaveLength(0)
      await persona.reload()
      // The person must learn what happened to the change they approved (or that it is being checked).
      await expect(page.locator("body")).toContainText(/Set stock levels|Raw Honey 500g/, { timeout: 8_000 })
      // Office's answer decides what the card says: confirmed only when it completed.
      await page.goto("/?view=task")
      await expect(page.locator("[data-receipt]")).toHaveAttribute(
        "data-receipt",
        officeSays === "completed" ? "completed" : "unknown"
      )
      await persona.check()
      // The route stays across the reload, so a second send would be counted here too.
      expect(sent, "the approval left once, never again after the reload").toHaveLength(1)
      expect(confirms.length, "Office got it at most once").toBeLessThanOrEqual(1)
    })
})

test.describe("P6-4: the socket drops while a tool is still answering", () => {
  // Fixed: regression guard.
  test("P6-4: the late tool result goes into a closed socket (console errors, misleading alert)", async ({ persona, page }) => {
    await page.goto("/")
    await startText(page)
    await delay(page, /\/api\/voice\/tools/, 2500)
    persona.agent.call("products_list", {})
    await page.waitForTimeout(300)
    persona.agent.close()
    await expect(page.locator("[data-phase]").first()).toHaveAttribute("data-phase", "ended")
    await page.waitForTimeout(3500)
    console.log("[probe] P6-4 alert:", await page.getByRole("alert").allInnerTexts())
    await persona.check()
    // The session already ended; a tool answering afterwards is not a new failure to report.
    expect(await alertText(page)).not.toContain("could not be completed")
  })
})

/** Conversation up to an open stock review (120). */
async function openReview(persona: Persona, page: Page, api: import("../../office/fake-api").FakeApi) {
  const stock = readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
  api.on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => stock.body as object)
  const confirms = await stockRoutes(page)
  await page.goto("/")
  await startText(page)
  persona.agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 120 }] },
  })
  await expect(scene(page)).toContainText("Confirm action 135790")
  return confirms
}

test.describe("P6-1d: approving offline shows the raw error too", () => {
  // Fixed: regression guard.
  test("P6-1d: Approve in a tunnel", async ({ persona, page, api }) => {
    await openReview(persona, page, api)
    await (await shortTunnel(page))(true)
    await scene(page).getByRole("button", { name: v.approve }).click()
    await expect(page.getByRole("alert").first()).not.toBeEmpty()
    expect(await alertText(page)).not.toMatch(RAW)
  })
})

test.describe("P6-5: approving while offline strands the review", () => {
  // Passes on flow/stress-int-1 (2026-11-12): kept as a regression guard.
  test("P6-5: nothing was sent, yet the card says it may have happened and never recovers", async ({ persona, page, api }) => {
    const confirms = await openReview(persona, page, api)
    const line = await shortTunnel(page)
    await line(true)
    await scene(page).getByRole("button", { name: v.approve }).click()
    await page.waitForTimeout(1000)
    expect(confirms, "offline: nothing reached Office").toHaveLength(0)
    // The proposed value stays readable while the outcome is open.
    await expect(scene(page)).toContainText("120")
    await line(false)
    // Back online: the person can approve again, or the card settles by itself (no dead end).
    await expect(
      scene(page).getByRole("button", { name: v.approve }).or(page.locator("[data-receipt-new]")),
      "back online, the review is stranded"
    ).toBeVisible({ timeout: 8_000 })
  })
})

test.describe("P6-6: a link or Back to home in a tunnel leaves Flow for the browser's error page", () => {
  // Next reports the RSC fetch it could not make offline, before the offline guard keeps Flow.
  test.use({ allow: { consolePrefix: ["Failed to fetch RSC payload for "] } })
  test("P6-6a: Back to home from a scene, offline", async ({ persona, page }) => {
    await page.goto("/scenes/orders")
    await expect(page.getByRole("button", { name: shell.backHome })).toBeVisible()
    await page.getByLabel(v.message).fill("Draft I am still typing")
    await persona.offline(page.context(), true)
    await page.getByRole("button", { name: shell.backHome }).click({ noWaitAfter: true, timeout: 5_000 }).catch(() => {})
    await page.waitForTimeout(1500)
    expect(page.url(), "Flow is gone (browser error page)").toMatch(/^http/)
    await expect(page.getByRole("region", { name: shell.dock })).toBeVisible()
    await expect(page.getByLabel(v.message)).toHaveValue("Draft I am still typing")
  })
  test("P6-6b: open a scene from Your Flow, offline", async ({ persona, page }) => {
    await page.goto("/")
    await expect(page.getByRole("region", { name: sc.yourFlow })).toBeVisible()
    await persona.offline(page.context(), true)
    await persona.click(page.getByRole("region", { name: sc.yourFlow }).getByRole("link").first())
    await page.waitForTimeout(1500)
    expect(page.url(), "Flow is gone (browser error page)").toMatch(/^http/)
    await expect(page.getByRole("region", { name: shell.dock })).toBeVisible()
  })
})
