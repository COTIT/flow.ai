// Persona 2, the impatient double-clicker: double- and triple-clicks everything, re-sends before
// the first answer, hammers Approve / Cancel / Pause / Continue, spams Back, Enter and the orb,
// and acts again while spinners show. Hunts duplicate writes, double navigation, stuck busy
// states, lost or doubled cards, race errors. SIMULATED API and voice (see harness.ts).
import type { Locator, Page, Route } from "@playwright/test"

import { sc } from "../scenes-api"
import { phase, revealHeader, s as shell } from "../shell"
import {
  ACTION,
  capsule,
  k,
  proposeStock as openStockReview,
  readBack,
  receipt,
  scene,
  stockReview,
  stockRoutes as baseStockRoutes,
} from "../task"
import { startText, v, type Agent, typeInstead } from "../voice"
import { expect, test, type Persona, type Step } from "./harness"

const many = (p: Persona) => p.pick(["double", "triple"] as const)
const userMessages = (agent: Agent) =>
  agent.sent.filter((e) => e.type === "user_message").map((e) => String(e.text))
/** Flow's own history depth (entries a Back can walk). */
const depth = (page: Page) => page.evaluate(() => history.length)

/** The stock receipt reads the product back after an approved change. */
function stockReadBack(api: { on: (m: string, p: string, f: () => object) => unknown }) {
  const stock = readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
  api.on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => stock.body as object)
}

/**
 * The task mock's stock routes, but every proposal gets its own ticket (Office never reuses one);
 * the shared mock names tickets by total only, so two proposals of 130 would look like one.
 */
async function stockRoutes(page: Page, confirmAnswer?: (ticket: string) => Record<string, unknown>) {
  const confirms = await baseStockRoutes(page, confirmAnswer)
  let n = 0
  await page.route("**/api/voice/tools", async (route) => {
    const b = (route.request().postDataJSON() ?? {}) as Record<string, unknown>
    if (b.command !== "execute" || b.operationId !== "stock_set_levels") return route.fallback()
    const total = (b.arguments as { body: { warehouses: { stock_lvl: number }[] } }).body.warehouses[0]!.stock_lvl
    return route.fulfill({ json: stockReview(total, `stock-${total}-${++n}`) })
  })
  return confirms
}

const proposeStock = (agent: Agent, total: number) =>
  agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: total }] },
  })

// ---- batch (Office's batch shapes, as in task-supplier.spec) -------------------------------
const steps = (done: number, total = 6) =>
  Array.from({ length: total }, (_, i) => ({
    status: i < done ? "completed" : "not_started",
    data: { actionId: `a-${i}`, operation: "products_update", summary: `Price ${i}` },
  }))
const batchState = (state: string, done: number) => ({
  status: "partial",
  data: { kind: "batch", batchId: "b-1", state, nextIndex: done, recoverable: false, steps: steps(done) },
})
/** Batch routes: prepare → review; confirm hangs until a stop; pause/cancel answer paused. */
async function batchRoutes(page: Page) {
  const commands: string[] = []
  const confirms: unknown[] = []
  let release: () => void = () => {}
  const stopped = new Promise<void>((resolve) => (release = resolve))
  await page.route("**/api/voice/batch", async (route: Route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>
    commands.push(String(body.command))
    if (body.command === "prepare")
      return route.fulfill({
        json: {
          status: "requires_confirmation",
          data: { batchId: "b-1" },
          review: {
            ticket: "batch-6",
            kind: "batch",
            batchId: "b-1",
            summary: "Update 6 supplier purchase prices",
            confirmationCode: "246810",
            confirmationPhrase: "Confirm batch 246810",
            details: { notIncluded: [] },
            steps: steps(0).map((s) => ({
              actionId: s.data.actionId,
              operationId: "products_update",
              summary: s.data.summary,
              details: {},
            })),
            expiresAt: new Date(Date.now() + 10 * 60_000).toISOString(),
          },
        },
      })
    if (body.command === "status") return route.fulfill({ json: batchState("running", 2) })
    if (body.command === "pause" || body.command === "cancel") {
      release()
      return route.fulfill({ json: batchState(body.command === "pause" ? "paused" : "cancelled", 3) })
    }
    return route.fallback()
  })
  await page.route("**/api/voice/confirm", async (route) => {
    confirms.push(route.request().postDataJSON())
    await stopped // still running when the person presses Pause
    return route.fulfill({ json: batchState("paused", 3) })
  })
  return { commands, confirms }
}

/**
 * How a person double-clicks: the pointer stays put and each click comes 60-250 ms after the
 * last (seeded). Whatever is under the pointer by then gets the click, as on a real screen.
 */
async function tap(persona: Persona, target: Locator, how: "single" | "double" | "triple" = "double") {
  const el = target.first()
  if (!(await el.isVisible().catch(() => false))) return false
  if (!(await el.isEnabled().catch(() => false))) return false
  await el.scrollIntoViewIfNeeded({ timeout: 1_000 }).catch(() => {})
  const box = await el.boundingBox().catch(() => null)
  if (!box) return false
  const x = box.x + box.width / 2
  const y = box.y + box.height / 2
  const page = persona.page
  for (let i = how === "single" ? 1 : how === "double" ? 2 : 3; i > 0; i--) {
    await page.mouse.click(x, y)
    if (i > 1) await page.waitForTimeout(persona.int(60, 250))
  }
  return true
}

/**
 * Double/triple-clicks one random visible control (never logout or links leaving Flow). Unlike
 * the harness's clickAnything, every read has a timeout: controls vanish mid-step here.
 */
async function hammerAnything(persona: Persona, scope: Locator, how?: "double" | "triple") {
  const controls = scope.locator('button:visible:not([disabled]), a[href]:visible:not([target="_blank"])')
  const count = await controls.count().catch(() => 0)
  if (!count) return
  const el = controls.nth(persona.int(0, count - 1))
  const name = (await el.textContent({ timeout: 1_000 }).catch(() => "")) ?? ""
  if (/log out|uitloggen|ondoka/i.test(name)) return
  persona.log(`   → ${name.trim().slice(0, 40)}`)
  await tap(persona, el, how ?? (persona.chance(0.5) ? "double" : "triple"))
}

/** Every write the browser sends to /v1 (method, path, body, when). */
function writeLog(page: Page) {
  const seen: { key: string; at: number }[] = []
  page.on("request", (request) => {
    const url = new URL(request.url())
    if (request.method() === "GET" || !url.pathname.startsWith("/v1/")) return
    seen.push({ key: `${request.method()} ${url.pathname} ${request.postData() ?? ""}`, at: Date.now() })
  })
  return seen
}
/** One intent, one write: the same write twice within a second is a double-click that got through. */
function expectNoRepeatedWrites(seen: { key: string; at: number }[], known?: RegExp) {
  const repeated = seen.filter(
    (w, i) => !known?.test(w.key) && seen.slice(0, i).some((p) => p.key === w.key && w.at - p.at < 1_000)
  )
  expect(repeated.map((w) => w.key), "the same write sent twice within a second").toEqual([])
}

// ---- the long session -----------------------------------------------------------------------

/** Steps per walk: PERSONA_STEPS wins; CI runs a short walk (suite budget), local runs the full one. */
const STEPS = Number(process.env.PERSONA_STEPS) || (process.env.CI ? 12 : 40)

test("double-clicker: everything twice or three times, before the last one finished", async ({
  persona,
  page,
  api,
}) => {
  stockReadBack(api)
  await stockRoutes(page)
  await page.goto("/")
  const live = () => page.getByRole("button", { name: v.addFiles }).isVisible()
  const sceneLinks = () => page.getByRole("region", { name: sc.yourFlow }).getByRole("link")

  const writes = writeLog(page)
  const steps: Step[] = [
    {
      name: "open a scene (double/triple)",
      weight: 3,
      run: async () => {
        const n = await sceneLinks().count()
        if (n) await tap(persona, sceneLinks().nth(persona.int(0, n - 1)), many(persona))
      },
    },
    { name: "back home (double/triple)", weight: 3, run: () => tap(persona, page.getByRole("button", { name: shell.backHome }), many(persona)) },
    { name: "storefront (double)", weight: 1, run: () => tap(persona, page.getByRole("link", { name: "Storefront", exact: true }), "double") },
    { name: "orb (double/triple)", weight: 3, run: () => tap(persona, page.locator("[data-orb]:not([aria-hidden])"), many(persona)) },
    { name: "use text (triple)", weight: 2, run: () => tap(persona, page.getByRole("button", { name: typeInstead, exact: true }), "triple") },
    {
      name: "ask, then Enter again and again",
      weight: 4,
      run: async () => {
        const box = page.getByLabel(v.message)
        if (!(await box.isEditable().catch(() => false))) return
        await box.fill(persona.pick(["Sales today?", "Which orders wait?", "Stock below minimum"]))
        for (let i = persona.int(2, 4); i > 0; i--) await box.press("Enter").catch(() => {})
      },
    },
    {
      name: "ask, then Send twice",
      weight: 2,
      run: async () => {
        const box = page.getByLabel(v.message)
        if (!(await box.isEditable().catch(() => false))) return
        await box.fill("Thanks")
        await tap(persona, page.getByRole("button", { name: v.send, exact: true }), many(persona))
      },
    },
    {
      name: "a stock change to review (agent proposes)",
      weight: 2,
      run: async () => {
        if (await live()) proposeStock(persona.agent, persona.pick([110, 120, 130]))
      },
    },
    { name: "approve (double/triple)", weight: 3, run: () => tap(persona, scene(page).getByRole("button", { name: v.approve }), many(persona)) },
    { name: "cancel (double)", weight: 1, run: () => tap(persona, scene(page).getByRole("button", { name: v.cancel, exact: true }), "double") },
    {
      name: "approve and cancel together",
      weight: 1,
      run: async () => {
        const s = scene(page)
        await Promise.all([
          tap(persona, s.getByRole("button", { name: v.approve }), "double"),
          tap(persona, s.getByRole("button", { name: v.cancel, exact: true }), "double"),
        ])
      },
    },
    { name: "capsule Review (double)", weight: 2, run: () => tap(persona, capsule(page).getByRole("button", { name: k.review }), many(persona)) },
    { name: "capsule View result (double)", weight: 1, run: () => tap(persona, capsule(page).getByRole("button", { name: k.openReceipt }), many(persona)) },
    { name: "capsule Dismiss (triple)", weight: 1, run: () => tap(persona, capsule(page).getByRole("button", { name: k.dismiss }), "triple") },
    {
      name: "Back spam",
      weight: 2,
      run: async () => {
        for (let i = persona.int(2, 4); i > 0; i--) await persona.history("back")
      },
    },
    { name: "Forward twice", weight: 1, run: async () => { await persona.history("forward"); await persona.history("forward") } },
    { name: "anything in main, double/triple", weight: 3, run: () => hammerAnything(persona, page.locator("main")) },
    { name: "a suggestion pill, double", weight: 2, run: () => hammerAnything(persona, page.getByRole("list", { name: shell.suggested }), "double") },
    { name: "theme toggle (triple)", weight: 1, run: () => tap(persona, page.getByRole("button", { name: /light or dark/i }), "triple") },
    {
      name: "a tool request fails once, then propose and approve",
      weight: 1,
      run: async () => {
        if (!(await live())) return
        await persona.failApi(/\/api\/voice\/tools$/)
        proposeStock(persona.agent, persona.pick([110, 120, 130]))
        await page.waitForTimeout(300)
        proposeStock(persona.agent, persona.pick([110, 120, 130]))
        await tap(persona, scene(page).getByRole("button", { name: v.approve }), "double")
      },
    },
    { name: "Enter on whatever has focus (x3)", weight: 1, run: () => persona.keys("Enter", "Enter", "Enter") },
    { name: "reload while busy", weight: 1, run: () => persona.reload() },
  ]
  await persona.run(
    steps.map((step) => ({
      ...step,
      run: async () => {
        await step.run()
        expectNoRepeatedWrites(writes, KNOWN_P2_3)
      },
    })),
    { count: STEPS }
  )
})

// ---- targeted races -------------------------------------------------------------------------

test("P2: Use text instead, triple-clicked, opens one conversation", async ({ persona, page }) => {
  await page.goto("/")
  await page.getByRole("button", { name: typeInstead, exact: true }).click({ clickCount: 3 })
  await expect(page.getByRole("button", { name: v.addFiles })).toBeVisible()
  await page.waitForTimeout(500)
  expect(persona.agent.connections, "one socket").toBe(1)
  await persona.check()
})

test("P2: Enter pressed three times sends the message once", async ({ persona, page }) => {
  await page.goto("/")
  await startText(page)
  const box = page.getByLabel(v.message)
  await box.fill("Sales today?")
  await box.press("Enter")
  await box.press("Enter")
  await box.press("Enter")
  await expect(page.getByRole("log")).toContainText("Noted: Sales today?")
  await page.waitForTimeout(400)
  expect(userMessages(persona.agent)).toEqual(["Sales today?"])
  await persona.check()
})

test("P2: Send clicked three times sends the message once", async ({ persona, page }) => {
  await page.goto("/")
  await startText(page)
  await page.getByLabel(v.message).fill("Which orders wait?")
  await page.getByRole("button", { name: v.send, exact: true }).click({ clickCount: 3 })
  await expect(page.getByRole("log")).toContainText("Noted: Which orders wait?")
  await page.waitForTimeout(400)
  expect(userMessages(persona.agent)).toEqual(["Which orders wait?"])
  await persona.check()
})

test("P2: a typed line sent from home before the conversation is up starts once and sends once", async ({
  persona,
  page,
}) => {
  await page.goto("/")
  const box = page.getByLabel(v.message)
  await box.fill("Stock below minimum")
  await box.press("Enter")
  await box.press("Enter")
  await box.press("Enter")
  await expect(page.getByRole("log")).toContainText("Noted: Stock below minimum")
  await page.waitForTimeout(500)
  expect(persona.agent.connections, "one socket").toBe(1)
  expect(userMessages(persona.agent)).toEqual(["Stock below minimum"])
  await persona.check()
})

test("P2: Approve triple-clicked and Enter-hammered decides once", async ({ persona, page, api }) => {
  stockReadBack(api)
  const confirms = await stockRoutes(page)
  await page.goto("/")
  await startText(page)
  proposeStock(persona.agent, 120)
  const approve = scene(page).getByRole("button", { name: v.approve })
  await approve.focus()
  await approve.click({ clickCount: 3 })
  await page.keyboard.press("Enter")
  await page.keyboard.press("Enter")
  await expect(receipt(page)).toHaveAttribute("data-receipt", "completed")
  await page.waitForTimeout(300)
  expect(confirms.map((c) => c.decision)).toEqual(["approve"])
  await persona.check()
})

test("P2: Approve then Cancel in the same breath sends one decision", async ({ persona, page, api }) => {
  stockReadBack(api)
  const confirms = await stockRoutes(page)
  await page.goto("/")
  await startText(page)
  proposeStock(persona.agent, 130)
  const s = scene(page)
  await expect(s.getByRole("button", { name: v.approve })).toBeEnabled()
  // Two clicks dispatched in one task: no render between them.
  await s.evaluate((el) => {
    const buttons = [...el.querySelectorAll("button")]
    const approve = buttons.find((b) => b.textContent?.trim() === "Yes, do it")
    const cancel = buttons.find((b) => b.textContent?.trim() === "Cancel")
    approve?.click()
    cancel?.click()
    approve?.click()
  })
  await expect(receipt(page)).toBeVisible()
  await page.waitForTimeout(300)
  expect(confirms.map((c) => c.decision)).toHaveLength(1)
  await persona.check()
})

test("P2: capsule Review double-clicked from another page is one step in history", async ({
  persona,
  page,
}) => {
  await stockRoutes(page)
  await page.goto("/")
  await startText(page)
  proposeStock(persona.agent, 110)
  await expect(scene(page)).toContainText("Confirm action 135790")
  await revealHeader(page)
  await page.getByRole("link", { name: "Storefront", exact: true }).click()
  await expect(page).toHaveURL(/\/storefront$/)
  const before = await depth(page)
  await capsule(page).getByRole("button", { name: k.review }).dblclick()
  await expect(page).toHaveURL(/\/\?view=task$/)
  await page.waitForTimeout(500)
  expect(await depth(page), "one history entry for one intent").toBe(before + 1)
  await page.goBack()
  await expect(page).toHaveURL(/\/storefront$/)
  await persona.check()
})

test("P2: a scene link double-clicked is one step in history", async ({ persona, page }) => {
  await page.goto("/")
  const link = page.getByRole("region", { name: sc.yourFlow }).getByRole("link").first()
  await expect(link).toBeVisible()
  const before = await depth(page)
  await link.dblclick()
  await expect(page).toHaveURL(/\/scenes\//)
  await page.waitForTimeout(500)
  expect(await depth(page)).toBe(before + 1)
  await page.goBack()
  await expect(page).toHaveURL(/\/$/)
  await persona.check()
})

test("P2: Back to home triple-clicked in a scene is one step in history", async ({ persona, page }) => {
  await page.goto("/")
  await page.getByRole("region", { name: sc.yourFlow }).getByRole("link").first().click()
  await expect(page).toHaveURL(/\/scenes\//)
  const sceneUrl = page.url()
  const before = await depth(page)
  await page.getByRole("button", { name: shell.backHome }).click({ clickCount: 3 })
  await expect(page).toHaveURL(/\/$/)
  await page.waitForTimeout(500)
  expect(await depth(page)).toBe(before + 1)
  await page.goBack()
  await expect(page).toHaveURL(sceneUrl)
  await persona.check()
})

test("P2: the compact orb triple-clicked in a scene is one step in history", async ({ persona, page }) => {
  await page.goto("/")
  await startText(page)
  await revealHeader(page)
  await page.getByRole("link", { name: "Storefront", exact: true }).click()
  await expect(page).toHaveURL(/\/storefront$/)
  const orbButton = page.locator('[data-orb="compact"]')
  await expect(orbButton).toBeVisible()
  const before = await depth(page)
  await orbButton.click({ clickCount: 3 })
  await expect(page).toHaveURL(/\/$/)
  await page.waitForTimeout(500)
  expect(await depth(page)).toBe(before + 1)
  await persona.check()
})

/** Up to a paused batch (3 of 6 done) whose rest waits for "Prepare the rest". */
async function pausedBatch(persona: Persona, page: Page) {
  const routes = await batchRoutes(page)
  await page.goto("/")
  await startText(page)
  persona.agent.call("assistant_batch_prepare", { calls: [{}, {}] })
  const summary = page.locator("[data-batch]")
  await expect(summary).toContainText("6 changes matched")
  await scene(page).getByRole("button", { name: v.approve }).click({ clickCount: 3 })
  const pause = scene(page).getByRole("button", { name: k.pause }).first()
  await expect(pause).toBeVisible()
  await pause.click({ clickCount: 3 }).catch(() => {})
  await scene(page).getByRole("button", { name: v.batchStop }).click({ timeout: 1_000 }).catch(() => {})
  await expect(summary).toHaveAttribute("data-batch", "paused")
  return { ...routes, summary }
}

test("P2: batch Pause hammered, then Stop: approved once, stopped once", async ({ persona, page }) => {
  const { confirms, commands } = await pausedBatch(persona, page)
  expect(confirms, "approved once").toHaveLength(1)
  expect(commands.filter((c) => c === "pause" || c === "cancel"), "one stop for one intent").toHaveLength(1)
  await persona.check()
})

// P2-1: "Prepare the rest" sends its request once per click; the button is disabled only while
// `voice.busy`, which ends as soon as the socket send returns (before the next click lands).
test("P2-1: batch Prepare the rest, double-clicked, asks Genesy once", async ({ persona, page }) => {
  const { summary } = await pausedBatch(persona, page)
  await summary.getByRole("button", { name: k.batch.continue }).dblclick()
  await page.waitForTimeout(600)
  expect(
    userMessages(persona.agent).filter((m) => m.startsWith("Prepare a new review")),
    "Continue sends one request"
  ).toHaveLength(1)
})

// P2-1 (same root cause): the receipt's "Ask Genesy to check" on an unconfirmed outcome.
test("P2-1: receipt Ask Genesy to check, double-clicked, asks once", async ({ persona, page, api }) => {
  stockReadBack(api)
  await stockRoutes(page, () => ({ status: "unknown", data: {} }))
  await page.goto("/")
  await startText(page)
  proposeStock(persona.agent, 120)
  await scene(page).getByRole("button", { name: v.approve }).click()
  await expect(receipt(page)).toHaveAttribute("data-receipt", "unknown")
  await receipt(page).getByRole("button", { name: k.receipt.check }).dblclick()
  await page.waitForTimeout(600)
  expect(userMessages(persona.agent).filter((m) => m === k.receipt.checkMessage)).toHaveLength(1)
})

// P2-1 check: the resume prompt's Continue restores through `voice.restore`, which holds `busy`
// until Office answers, so a double-click must reach Office once and open no new voice session.
test("P2-1: resume Continue, double-clicked, restores once", async ({ persona, page }) => {
  await stockRoutes(page)
  const commands: string[] = []
  await page.route("**/api/voice/tools", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>
    if (body.command === "status")
      return route.fulfill({
        json: {
          status: "completed",
          data: [
            {
              status: "awaiting_review",
              data: {
                actionId: ACTION(120),
                operation: "stock_set_levels",
                summary: "Set stock levels per warehouse",
                recoverable: true,
              },
            },
          ],
        },
      })
    if (body.command === "restore") {
      commands.push(String(body.command))
      await new Promise((resolve) => setTimeout(resolve, 300))
      return route.fulfill({ json: stockReview(120, "stock-120-restored") })
    }
    return route.fallback()
  })
  await openStockReview(page, persona.agent, 120)
  persona.agent.close()
  await expect(phase(page)).toHaveAttribute("data-phase", "ended")
  const prompt = page.getByRole("region", { name: k.resume.title })
  await prompt.getByRole("button", { name: k.resume.continue }).dblclick()
  await expect(scene(page)).toContainText(v.recoveredReviewHint)
  await page.waitForTimeout(600)
  expect(commands, "one restore for one Continue").toHaveLength(1)
  expect(persona.agent.connections, "no new voice session").toBe(1)
  await persona.check()
})

test("P2: receipt Dismiss and View result hammered", async ({ persona, page, api }) => {
  stockReadBack(api)
  await stockRoutes(page)
  await page.goto("/")
  await startText(page)
  proposeStock(persona.agent, 120)
  await scene(page).getByRole("button", { name: v.approve }).click()
  await expect(receipt(page)).toHaveAttribute("data-receipt", "completed")
  await page.getByRole("button", { name: shell.backHome }).dblclick()
  await capsule(page).getByRole("button", { name: k.openReceipt }).click({ clickCount: 3 })
  await expect(receipt(page)).toBeVisible()
  await page.getByRole("button", { name: shell.backHome }).click()
  await capsule(page).getByRole("button", { name: k.dismiss }).click({ clickCount: 3 })
  await expect(capsule(page)).toHaveCount(0)
  await persona.check()
})

test("P2: Clarify Prepare change, double-clicked, asks once", async ({ persona, page }) => {
  await stockRoutes(page)
  await page.goto("/")
  await startText(page)
  await page.getByLabel(v.message).fill("Update the honey stock.")
  await page.getByLabel(v.message).press("Enter")
  await expect(page.getByRole("log")).toContainText("Noted: Update the honey stock.")
  persona.agent.call("products_list", { query: { q: "honey" } })
  await expect.poll(persona.agent.results).toBe(1)
  persona.agent.say("I found three honey products. Which one did you mean?")
  await capsule(page).getByRole("button", { name: k.review }).click()
  const choice = page.getByRole("form", { name: k.clarify.title })
  await choice.getByRole("radio", { name: /Raw Honey 500g/ }).check()
  await choice.getByLabel(k.clarify.type).selectOption({ label: k.clarify.types.set })
  await choice.getByLabel(k.clarify.quantity).fill("120")
  await choice.getByRole("button", { name: k.clarify.prepare }).click({ clickCount: 3 })
  // A line sent while the agent still answers is held until its turn settles (~1.5 s).
  const sets = () => userMessages(persona.agent).filter((m) => m.includes("Set the stock")).length
  await expect.poll(sets, { timeout: 5_000 }).toBeGreaterThan(0)
  await page.waitForTimeout(2_000)
  expect(sets()).toBe(1)
  await persona.check()
})

test("P2: while an approval is on its way: Approve, Back, Review, End, all twice", async ({
  persona,
  page,
  api,
}) => {
  stockReadBack(api)
  const confirms = await stockRoutes(page)
  // The decision takes a while to come back.
  await page.route("**/api/voice/confirm", async (route) => {
    await new Promise((r) => setTimeout(r, 2_000))
    return route.fallback()
  })
  await page.goto("/")
  await startText(page)
  proposeStock(persona.agent, 120)
  const approve = scene(page).getByRole("button", { name: v.approve })
  await approve.dblclick()
  await persona.click(approve, "double")
  await persona.click(page.getByRole("button", { name: shell.backHome }), "double")
  await persona.click(capsule(page).getByRole("button", { name: k.review }), "double")
  await persona.click(page.getByRole("button", { name: v.end }), "double")
  await expect.poll(() => confirms.length, { timeout: 5_000 }).toBeGreaterThan(0)
  await page.waitForTimeout(2_500)
  expect(confirms.map((c) => c.decision)).toEqual(["approve"])
  await persona.check()
})

test("P2: End conversation double-clicked, then Use text instead hammered", async ({ persona, page }) => {
  await page.goto("/")
  await startText(page)
  await page.getByRole("button", { name: v.end }).dblclick()
  await persona.click(page.getByRole("button", { name: typeInstead, exact: true }), "triple")
  await persona.click(page.getByRole("button", { name: typeInstead, exact: true }), "double")
  await expect(page.getByRole("button", { name: v.addFiles })).toBeVisible()
  await page.waitForTimeout(500)
  expect(persona.agent.connections, "one new socket after the first").toBe(2)
  await persona.check()
})

/** Up to the agent's "which product?" with the choice waiting (task state waiting_input). */
async function askWhichProduct(persona: Persona, page: Page) {
  await stockRoutes(page)
  await page.goto("/")
  await startText(page)
  await page.getByLabel(v.message).fill("Update the honey stock.")
  await page.getByLabel(v.message).press("Enter")
  await expect(page.getByRole("log")).toContainText("Noted: Update the honey stock.")
  persona.agent.call("products_list", { query: { q: "honey" } })
  await expect.poll(persona.agent.results).toBe(1)
  persona.agent.say("I found three honey products. Which one did you mean?")
  await expect(capsule(page)).toHaveAttribute("data-task-state", "waiting_input")
}

// P2-2: the dock's task button opens the task scene on the first click; the dock then shows a
// suggestion pill (Morning briefing / Check pending orders) where the button was, and the second
// click of the double-click opens that scene instead. The person lands in an unrelated scene.
test("P2-2: View result double-clicked opens the receipt, not another scene", async ({ persona, page, api }) => {
  stockReadBack(api)
  await stockRoutes(page)
  await page.goto("/")
  await startText(page)
  proposeStock(persona.agent, 120)
  await scene(page).getByRole("button", { name: v.approve }).click()
  await expect(receipt(page)).toBeVisible()
  await page.getByRole("button", { name: shell.backHome }).click()
  const open = capsule(page).getByRole("button", { name: k.openReceipt })
  await expect(open).toBeVisible()
  await page.waitForTimeout(1_600) // the agent's turn has settled; the suggestions are live
  await open.dblclick()
  await page.waitForTimeout(1_000)
  await expect(page).toHaveURL(/\/\?view=task$/)
  await expect(receipt(page)).toBeVisible()
})

test("P2-2: Review double-tapped while Genesy waits for a choice opens the choice", async ({ persona, page }, info) => {
  test.skip(info.project.name !== "flow-mobile", "the pill sits under the button at 375 only")
  await askWhichProduct(persona, page)
  await capsule(page).getByRole("button", { name: k.review }).dblclick()
  await page.waitForTimeout(1_000)
  await expect(page).toHaveURL(/\/\?view=task$/)
  await expect(page.getByRole("form", { name: k.clarify.title })).toBeVisible()
})

/** P2-3: scene buttons stay enabled after a fast success; a second click repeats the same PUT. */
const KNOWN_P2_3 = /^PUT \/v1\/workspace\/[^/]+\/flow\/scenes\//

/** Each write button on Morning Pulse clicked several times; returns the writes seen. */
async function hammerPulse(persona: Persona, page: Page, click: (button: Locator) => Promise<unknown>) {
  const writes = writeLog(page)
  await page.goto("/scenes/pulse")
  const main = page.locator("main")
  // Lists board: in the bento the attention list may show its row menu (⋯), or (P3) Later ▾ beside
  // the row's next step, instead.
  await expect(main.getByRole("button", { name: /^(Mark as seen|Actions for .*|Later .*)$/ }).first()).toBeVisible()
  for (const name of [/^Mark as seen$/, /^Remind me tomorrow$/, /^Remind me in a week$/, /first$/, /Remove from Your Flow/]) {
    const button = main.getByRole("button", { name }).first()
    if (!(await button.isVisible().catch(() => false))) continue
    persona.log(`hammer ${name}`)
    await click(button)
    await page.waitForTimeout(1_000)
  }
  expect(writes.length, "something was written").toBeGreaterThan(0)
  return writes
}

test("P2: every write button on Morning Pulse, double-clicked by a person, writes once (bar P2-3)", async ({
  persona,
  page,
}) => {
  const writes = await hammerPulse(persona, page, (b) => tap(persona, b, "double"))
  expectNoRepeatedWrites(writes, KNOWN_P2_3)
  await persona.check()
})

// P2-3 (LOW): (a) clicks landing in the same frame (bouncy touch screens, switch devices,
// scripted input) pass `disabled={mutation.isPending}` before React re-renders: the same PUT goes
// twice; (b) scene buttons (Show pending orders first, Remove from Your Flow) stay enabled after
// a fast success, so a person's second click repeats the same (idempotent) scene PUT.
test("P2-3: same-frame clicks on Morning Pulse write buttons write once", async ({ persona, page }) => {
  const writes = await hammerPulse(persona, page, (b) => b.click({ clickCount: 2 }))
  expectNoRepeatedWrites(writes)
})
