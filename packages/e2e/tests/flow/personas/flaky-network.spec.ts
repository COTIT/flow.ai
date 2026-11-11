// Persona 6, the flaky-network commuter: the train goes through tunnels. Offline and online at
// random moments, slow 3G, requests dropped mid-flight, answers late or out of order, reloads
// while waiting, the voice socket lost mid-turn. SIMULATED API and voice (see harness.ts).
import type { CDPSession, Page } from "@playwright/test"

import { markInjected } from "../../../src/test"
import { sc } from "../scenes-api"
import { s as shell } from "../shell"
import { readBack, scene, stockRoutes } from "../task"
import { startText, v } from "../voice"
import { expect, test, type Persona } from "./harness"

const SCENES = ["pulse", "stock", "noticed", "orders", "sales", "routines"] as const
/** Raw codes a person must never read (HTTP statuses, error names, ids). */
const RAW = /\b(?:[45]\d\d|ERR_[A-Z_]+|TypeError|Failed to fetch|NetworkError|undefined|null|NaN)\b|is not valid JSON|Unexpected token/

/** Chrome DevTools "Slow 3G" (latency ms, bytes/s); off restores the line. */
async function throttle(cdp: CDPSession, on: boolean) {
  await cdp.send("Network.emulateNetworkConditions", {
    offline: false,
    latency: on ? 2000 : 0,
    downloadThroughput: on ? 50_000 : -1,
    uploadThroughput: on ? 50_000 : -1,
  })
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

async function rawOnScreen(page: Page) {
  const text = await page.locator("body").innerText().catch(() => "")
  return text.match(RAW)?.[0] ?? null
}

/** Shared tunnel steps for the walk. */
function tunnelSteps(persona: Persona, page: Page, cdp: CDPSession) {
  const context = page.context()
  let offline = false
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
    { name: "back to home", weight: 2, run: () => persona.backHome() },
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
      name: "no raw codes on screen",
      weight: 1,
      run: async () => expect(await rawOnScreen(page), "raw code on screen").toBeNull(),
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

test("flaky network: tunnels, slow 3G, dropped and late answers, lost socket", async ({ persona, page, api }) => {
  const stock = readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
  api.on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => stock.body as object)
  await stockRoutes(page)
  const cdp = await page.context().newCDPSession(page)
  await page.goto("/")
  await persona.run(tunnelSteps(persona, page, cdp))
  // Back on a good line, the app recovers by itself (or with one click).
  await persona.offline(page.context(), false)
  await throttle(cdp, false)
  await persona.check()
})

// ---- targeted probes (each one a commuter moment) ---------------------------------------------

/** Raw text visible anywhere in the shell. */
async function expectReadable(page: Page) {
  expect(await rawOnScreen(page), "raw error text on screen").toBeNull()
}

test("probe: approve while the confirm answer is lost in a tunnel", async ({ persona, page, api }) => {
  const stock = readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
  api.on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => stock.body as object)
  await stockRoutes(page)
  await page.goto("/")
  await startText(page)
  persona.agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 120 }] },
  })
  await expect(scene(page)).toContainText("Confirm action 135790")
  await dropOnce(page, /\/api\/voice\/confirm/)
  await scene(page).getByRole("button", { name: v.approve }).click()
  await persona.check()
  await expectReadable(page)
  console.log("[probe] after lost confirm:", (await page.locator("main").innerText()).slice(0, 600))
})

test("probe: start a conversation in a tunnel, then out of it", async ({ persona, page }) => {
  await page.goto("/")
  await expect(page.getByRole("button", { name: v.textInstead, exact: true })).toBeVisible()
  await persona.offline(page.context(), true)
  await persona.startText()
  await page.waitForTimeout(1500)
  await persona.check()
  await expectReadable(page)
  console.log("[probe] start offline:", await page.locator("[data-phase]").first().getAttribute("data-phase"), "\n", await page.getByRole("region", { name: shell.dock }).ariaSnapshot())
  await persona.offline(page.context(), false)
  await startText(page)
})

test("probe: the socket drops while a draft is typed", async ({ persona, page }) => {
  await page.goto("/")
  await startText(page)
  await page.getByLabel(v.message).fill("Half a thought about the honey order")
  persona.agent.close()
  await page.waitForTimeout(1000)
  await persona.check()
  await expectReadable(page)
  const box = page.getByLabel(v.message)
  console.log(
    "[probe] draft after drop:",
    (await box.count()) ? await box.inputValue().catch(() => "(no box)") : "(no box)",
    "|",
    (await page.locator("body").innerText()).slice(0, 400)
  )
})

test("probe: open a scene in a tunnel (client navigation)", async ({ persona, page }) => {
  await page.goto("/")
  await expect(page.getByRole("region", { name: sc.yourFlow })).toBeVisible()
  await persona.offline(page.context(), true)
  await persona.click(page.getByRole("region", { name: sc.yourFlow }).getByRole("link").first())
  await page.waitForTimeout(1000)
  console.log("[probe] scene offline:", page.url(), (await page.locator("body").innerText()).slice(0, 400))
  await persona.check()
  await persona.offline(page.context(), false)
  await persona.check()
})

test("probe: a scene stays open through a long tunnel; the label tells the truth", async ({ persona, page }) => {
  await page.clock.install()
  await page.goto("/scenes/orders")
  await expect(page.locator("[data-freshness]")).toHaveAttribute("data-freshness", "fresh")
  await persona.offline(page.context(), true)
  await page.clock.fastForward(150_000)
  const label = page.locator("[data-freshness]")
  console.log("[probe] offline 150s:", await label.getAttribute("data-freshness"), await label.innerText())
  await persona.click(page.getByRole("button", { name: sc.refresh }))
  await page.clock.fastForward(5_000)
  console.log("[probe] offline refresh:", await label.getAttribute("data-freshness"), await label.innerText())
  await persona.check()
  await persona.offline(page.context(), false)
  await page.clock.fastForward(2_000)
  await expect(label).toHaveAttribute("data-freshness", "fresh")
})

test("probe: a tool answer is lost mid-turn", async ({ persona, page }) => {
  await page.goto("/")
  await startText(page)
  await dropOnce(page, /\/api\/voice\/tools/)
  persona.agent.call("products_list", {})
  await page.waitForTimeout(1500)
  await persona.check()
  console.log("[probe] tool drop:", await page.getByRole("alert").allInnerTexts(), persona.agent.sent.filter((e) => e.type === "client_tool_result").map((e) => JSON.stringify(e).slice(0, 300)))
  await expectReadable(page)
})

test("probe: the gateway answers an HTML 502 when starting", async ({ persona, page }) => {
  await page.goto("/")
  await expect(page.getByRole("button", { name: v.textInstead, exact: true })).toBeVisible()
  markInjected(page, /\/api\/voice\//)
  let left = 1
  await page.route(/\/api\/voice\/session/, (route) =>
    left-- > 0
      ? route.fulfill({ status: 502, contentType: "text/html", body: "<html><body>502 Bad Gateway</body></html>" })
      : route.fallback()
  )
  await persona.startText()
  await page.waitForTimeout(1500)
  await persona.check()
  console.log("[probe] 502 html:", await page.getByRole("alert").allInnerTexts())
  await expectReadable(page)
})

// ---- findings (red until fixed; ids reported to the coordinator) -----------------------------

/** The dock's alert, where the voice provider puts its errors. */
const alertText = async (page: Page) => (await page.getByRole("alert").allInnerTexts()).join(" | ")

test.describe("P6-1: a dropped or garbled voice request says so in plain words", () => {
  test("P6-1a: Use text instead while offline", async ({ persona, page }) => {
    await page.goto("/")
    await expect(page.getByRole("button", { name: v.textInstead, exact: true })).toBeVisible()
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
    await expect(page.getByRole("button", { name: v.textInstead, exact: true })).toBeVisible()
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

test.describe("P6-4: the socket drops while a tool is still answering", () => {
  test("P6-4: the late tool result goes into a closed socket (console errors, misleading alert)", async ({ persona, page }) => {
    await page.goto("/")
    await startText(page)
    await delay(page, /\/api\/voice\/tools/, 2500)
    persona.agent.call("products_list", {})
    await page.waitForTimeout(300)
    persona.agent.close()
    await expect(page.locator("[data-phase]").first()).toHaveAttribute("data-phase", "ended")
    await page.waitForTimeout(3500)
    await persona.check()
    // The session already ended; a tool answering afterwards is not a new failure to report.
    expect(await alertText(page)).not.toContain("could not be completed")
  })
})

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
      await delay(page, /\/api\/voice\/confirm/, 3000)
      markInjected(page, /\/api\/voice\/confirm/)
      await scene(page).getByRole("button", { name: v.approve }).click()
      // The approval is on the wire (Office will get it), the answer not back yet: the person reloads.
      await page.waitForTimeout(300)
      expect(confirms).toHaveLength(0)
      await persona.reload()
      // The person must learn what happened to the change they approved (or that it is being checked).
      await expect(page.locator("body")).toContainText(/Set stock levels|Raw Honey 500g/, { timeout: 8_000 })
      await persona.check()
      expect(confirms, "the approval reached Office once").toHaveLength(1)
    })
})
