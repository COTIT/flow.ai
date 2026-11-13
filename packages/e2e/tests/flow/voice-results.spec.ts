import type { Page } from "@playwright/test"

import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { type Agent, fakeAgent, startText } from "./voice"

// SIMULATED voice: mock upstream behind Flow's real proxy + a fake ElevenLabs socket. Office's
// view choice is unit-tested (apps/office/lib/voice/present.test.ts, routes.test.ts); here a
// page.route stands in for it, drawing the kind the agent asked for, so the round trip
// agent → Flow → "Office" → card is proven for one non-default kind of every data shape.

const ID = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b"
const SHAPES = [
  {
    shape: "series", op: "orders_sales_daily", kind: "sparkline",
    view: { kind: "sales_daily", from: "2026-11-03", to: "2026-11-06", time_zone: "UTC", currencies: [{ currency: "TZS", decimals: 0, days: [
      { date: "2026-11-03", gross: 120000, net: 100000, orders: 3 },
      { date: "2026-11-04", gross: 0, net: 0, orders: 0 },
      { date: "2026-11-05", gross: 450000, net: 380000, orders: 9 },
    ] }] },
  },
  {
    shape: "totals", op: "orders_sales_summary", kind: "kpi",
    view: { kind: "sales", from: "2026-11-01", to: "2026-11-08", source: null, rows: [{ currency: "TZS", decimals: 0, gross: 1250000, net: null, vat: null, refunded: 0, orders: 42 }] },
  },
  {
    shape: "comparison", op: "orders_sales_compare", kind: "bar",
    view: { kind: "comparison", from: "2026-11-01", to: "2026-11-08", base_from: "2026-10-25", base_to: "2026-11-01", currencies: [{ currency: "TZS", decimals: 0, current: 900000, base: 600000, change_bp: 5000 }] },
  },
  {
    shape: "breakdown", op: "products_top_sellers", kind: "pie",
    view: { kind: "breakdown", by: "products", from: "2026-11-01", to: "2026-11-08", currencies: [{ currency: "TZS", decimals: 0, rows: [
      { label: "Wild honey 500 g with a very long product name that has to fit", gross: 300000, orders: 6 },
      { label: "Beeswax candle", gross: 150000, orders: 3 },
      { label: "Propolis", gross: 120000, orders: 3 },
    ] }] },
  },
  {
    shape: "share", op: "lots_passport_get", kind: "donut",
    view: { kind: "lot_origin", title: "Lot 7", origin: null, materials: [{ name: "Honey", origin: null, share: 7500 }, { name: "Wax", origin: null, share: 2500 }] },
  },
  {
    shape: "counts", op: "attention_summary", kind: "hbar",
    view: { kind: "attention", sections: [{ key: "orders", counts: { open: 3, acknowledged: 1 }, items: [] }] },
  },
  {
    shape: "rows", op: "orders_list", kind: "kpi",
    view: { kind: "records", resource: "orders", total: 42, href: "/orders", rows: [] },
  },
  {
    shape: "events", op: "lots_events", kind: "table",
    view: { kind: "trail", events: [{ kind: "created", from: null, to: null, at: "2026-11-01", by: null }] },
  },
] as const

/** Stands in for Office: every listed read answers with its picture, drawn as asked. */
async function office(page: Page) {
  const seen: Record<string, unknown>[] = []
  await page.route("**/api/voice/tools", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>
    const known = SHAPES.find((s) => s.op === body?.operationId)
    if (!known) return route.fallback()
    seen.push(body)
    const asked = body.presentation as string | undefined
    // Office keeps the default for a kind the data cannot take (the sole refusal used here: pie on a series).
    const refused = asked === "pie" && known.shape === "series"
    await route.fulfill({
      json: {
        status: "completed",
        data: {},
        view: { ...known.view, ...(asked && !refused ? { presentation: asked } : {}) },
        ...(refused ? { presentation_note: { asked, shown: "line", closest: "bar", reason: "a pie needs parts of one whole" } } : {}),
        ...(body.replaces ? { replaces: body.replaces } : {}),
      },
    })
  })
  return seen
}

/** The agent asks for a read, optionally drawn as a kind and/or redrawing a picture in place. */
function ask(agent: Agent, operation: string, extra: Record<string, string> = {}) {
  agent.emit({
    type: "client_tool_call",
    client_tool_call: {
      tool_name: "execute_genesy_tool",
      tool_call_id: `call-${agent.sent.length}-${operation}`,
      parameters: { operation_id: operation, arguments_json: "{}", ...extra },
      expects_response: true,
    },
  })
}

function lastContext(agent: Agent) {
  const sent = agent.sent.filter((e) => e.type === "contextual_update" && String(e.text).includes("flow_context"))
  return (JSON.parse(String(sent.at(-1)?.text ?? "{}")) as { flow_context?: { pictures?: { id: string; shown_as: string }[] } }).flow_context
}

async function open(page: Page) {
  const seen = await office(page)
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  return { agent, seen }
}

// P5: answers show inline under the reply (each in its own card in the conversation).
const pane = (page: Page) => page.getByRole("log").first().locator("[data-inline-result]")
async function showResults(page: Page) {
  return pane(page)
}

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi())
})

for (const { shape, op, kind } of SHAPES)
  test(`${shape}: asked as ${kind}, the card draws it from the API's figures`, async ({ page }) => {
    const { agent, seen } = await open(page)
    ask(agent, op, { presentation: kind })
    await expect.poll(agent.results).toBe(1)
    expect(seen[0]).toMatchObject({ operationId: op, presentation: kind })
    const card = (await showResults(page)).locator('[data-role="result"]')
    if (kind === "table") await expect(card.getByRole("table")).toBeVisible()
    else await expect(card.locator(`[data-chart="${kind}"]`)).toBeVisible()
    if (kind === "pie" || kind === "donut") await expect(card.locator("svg[role=img] path")).toHaveCount(shape === "share" ? 2 : 3)
    // The agent hears how it is drawn, never the picture.
    const heard = JSON.parse(String(agent.sent.find((e) => e.type === "client_tool_result")?.result)) as Record<string, unknown>
    expect(heard).not.toHaveProperty("view")
    expect(heard.on_screen).toMatchObject({ shown_as: kind })
    await expectNoHorizontalScroll(page)
  })

test("\"make that a bar chart\" redraws the same picture in place", async ({ page }) => {
  const { agent, seen } = await open(page)
  ask(agent, "orders_sales_daily")
  await expect.poll(agent.results).toBe(1)
  const results = await showResults(page)
  await expect(results.locator('[data-chart="line"]')).toBeVisible()
  await expect.poll(() => lastContext(agent)?.pictures?.length).toBe(1)
  const id = lastContext(agent)!.pictures![0]!.id
  ask(agent, "orders_sales_daily", { presentation: "bar", replaces: id })
  await expect.poll(agent.results).toBe(2)
  expect(seen.at(-1)).toMatchObject({ presentation: "bar", replaces: id })
  await expect(results.locator('[data-role="result"]')).toHaveCount(1)
  await expect(results.locator('[data-chart="bar"]')).toBeVisible()
  await expect.poll(() => lastContext(agent)?.pictures).toEqual([expect.objectContaining({ id, shown_as: "bar" })])
  await expectNoHorizontalScroll(page)
})

test("a kind the data cannot take keeps the picture, and the agent is told why and the closest fit", async ({ page }) => {
  const { agent } = await open(page)
  ask(agent, "orders_sales_daily")
  await expect.poll(agent.results).toBe(1)
  const results = await showResults(page)
  await expect.poll(() => lastContext(agent)?.pictures?.length).toBe(1)
  ask(agent, "orders_sales_daily", { presentation: "pie", replaces: lastContext(agent)!.pictures![0]!.id })
  await expect.poll(agent.results).toBe(2)
  const told = agent.sent.filter((e) => e.type === "client_tool_result").at(-1)
  const heard = JSON.parse(String(told?.result)) as { on_screen: { instruction: string; shown_as: string } }
  expect(heard.on_screen.shown_as).toBe("line")
  expect(heard.on_screen.instruction).toMatch(/asked for a pie, which does not fit: a pie needs parts of one whole.*bar/)
  await expect(results.locator('[data-role="result"]')).toHaveCount(1)
  await expect(results.locator('[data-chart="line"]')).toBeVisible()
})

test("a picture that is no longer on screen is never removed: the answer is added", async ({ page }) => {
  const { agent } = await open(page)
  ask(agent, "orders_sales_daily")
  await expect.poll(agent.results).toBe(1)
  const results = await showResults(page)
  ask(agent, "products_top_sellers", { presentation: "donut", replaces: ID })
  await expect.poll(agent.results).toBe(2)
  await expect(results.locator('[data-role="result"]')).toHaveCount(2)
  await expect(results.locator('[data-chart="line"]')).toBeVisible()
  await expect(results.locator('[data-chart="donut"]')).toBeVisible()
})

// P6-2 (ported from the flaky-network persona, fb188b110): still one request at a time, but a
// read made while another runs waits and goes next instead of being refused; the newest waiting
// read wins the slot, an older waiting one is answered "superseded" at once.
test("P6-2: reads made while one is answering run in turn; the latest result shows the newest", async ({ page }) => {
  const agent = await fakeAgent(page)
  const asked: string[] = []
  await page.route("**/api/voice/tools", async (route) => {
    const body = route.request().postDataJSON() as { operationId?: string; arguments?: { query?: { q?: string } } }
    if (body?.operationId !== "products_list") return route.fallback()
    const q = body.arguments?.query?.q ?? "?"
    asked.push(q)
    if (q === "old") await new Promise((r) => setTimeout(r, 2500))
    await route.fulfill({
      json: {
        status: "completed",
        data: { q },
        view: {
          kind: "records", resource: "products", total: 1, href: "/products",
          rows: [{ id: ID, label: `${q.toUpperCase()} ANSWER`, detail: "x", status: null, money: null, stock: 1, href: `/products/${ID}` }],
        },
      },
    })
  })
  await page.goto("/")
  await startText(page)
  const read = (q: string) =>
    agent.emit({
      type: "client_tool_call",
      client_tool_call: {
        tool_name: "execute_genesy_tool",
        tool_call_id: `call-${q}`,
        parameters: { operation_id: "products_list", arguments_json: JSON.stringify({ query: { q } }) },
        expects_response: true,
      },
    })
  read("old")
  await page.waitForTimeout(200)
  read("middle")
  await page.waitForTimeout(100)
  read("new")
  await expect.poll(agent.results).toBe(3)
  const heard = Object.fromEntries(
    agent.sent
      .filter((e) => e.type === "client_tool_result")
      .map((e) => [String(e.tool_call_id), JSON.parse(String(e.result)) as Record<string, unknown>])
  )
  // Office ran one read at a time, in order; the superseded one never ran.
  expect(asked).toEqual(["old", "new"])
  expect(heard["call-middle"]).toEqual({ status: "superseded", message: "Replaced by a newer request" })
  expect(heard["call-old"]).toMatchObject({ status: "completed", data: { q: "old" } })
  expect(heard["call-new"]).toMatchObject({ status: "completed", data: { q: "new" } })
  for (const answer of Object.values(heard)) expect(JSON.stringify(answer)).not.toContain("Finish the current review")
  agent.say("Here are the products.")
  const results = await showResults(page)
  // Inline answers run oldest to newest: the newest is the last one.
  await expect(results.locator('[data-role="result"]').last()).toContainText("NEW ANSWER")
  await expect(page.getByText("Finish the current review first.")).toHaveCount(0)
  await expectNoHorizontalScroll(page)
})
