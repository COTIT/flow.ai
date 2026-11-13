import type { Page } from "@playwright/test"

import { resultView } from "../../../voice/src/lib/views"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { type Agent, fakeAgent, startText } from "./voice"

// SIMULATED voice: mock upstream behind Flow's real proxy + a fake ElevenLabs socket. Each read
// answers with the picture Office's server builds: resultView() over an API-shaped reply, so the
// parser and the card are proven together.

const A = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b"
const B = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5c"

const READS: { op: string; reply: unknown; sees: (string | RegExp)[] }[] = [
  {
    op: "stock_list",
    reply: {
      data: [
        { id: A, stock_lvl: "24.00", reserved: "4.00", available: "20.00", minimal_stock_level: 10, lot: { code: "L-2026-11" },
          relationships: { product: { id: A, name: "Wild forest honey 500 g", product_nr: "HON-500" }, warehouse: { name: "Main store" } } },
        { id: B, stock_lvl: "6.00", reserved: "0.00", available: "6.00", minimal_stock_level: 12, lot: null,
          relationships: { product: { id: B, name: "Beeswax candle, large, hand-poured with a very long name", product_nr: "WAX-L" }, warehouse: { name: "Main store" } } },
        { id: B, stock_lvl: "-2.00", reserved: "0.00", available: "-2.00", minimal_stock_level: 0, lot: null,
          relationships: { product: { id: B, name: "Propolis tincture", product_nr: "PRO-30" }, warehouse: { name: "Arusha shop" } } },
      ],
      meta: { total: 31 },
    },
    sees: ["Wild forest honey 500 g", "20", "-2", "Arusha shop", "4 reserved"],
  },
  {
    op: "stock_with_bins",
    reply: {
      data: [
        { id: A, stock_lvl: "18.00", reserved: "4.00", warehouse: { name: "Main store" }, lot: { code: "L-2026-11" },
          shelf: { shelf_name: "Top shelf", row: { row_name: "Aisle 2" } }, bin: { bin_name: "Box 4" } },
        { id: B, stock_lvl: "6.00", reserved: "0.00", warehouse: { name: "Main store" }, lot: null, shelf: null, bin: null },
        { id: B, stock_lvl: "3.00", reserved: "0.00", warehouse: { name: "Arusha shop" }, lot: null, shelf: null, bin: { bin_name: "Counter" } },
      ],
    },
    sees: ["Aisle 2 › Top shelf › Box 4", "18", "Counter"],
  },
  {
    op: "search_quick",
    reply: {
      data: {
        products: [{ id: A, name: "Wild forest honey 500 g", product_nr: "HON-500", ean13: "8712345678901" }],
        orders: [{ id: A, number: "ORD-0042", reference: "PO-77", status: "open", total: 125050, decimals: 2, currency: "USD" }],
        invoices: [{ id: B, number: "2026-0042", status: "paid", customer_name: "Neema Shop" }],
        lots: [],
        relations: [{ id: A, name: "Neema Shop" }],
      },
    },
    sees: ["ORD-0042", "$1,250.50", "Neema Shop"],
  },
  {
    op: "orders_get",
    reply: {
      data: {
        id: A, number: "ORD-0042", status: "open", customer_name: "Neema Shop", created_at: "2026-11-10T09:00:00Z",
        currency: "USD", decimals: 2, subtotal: 107800, vat: 17250, total: 125050, paid_at: null, picked_at: null,
        lines: [
          { title: "Wild forest honey 500 g", quantity: "10.00", unit_price: 9800, line_excl: 98000, line_vat: 15680 },
          { title: "Beeswax candle, large", quantity: "2.00", unit_price: 4900, line_excl: 9800, line_vat: 1570 },
        ],
      },
    },
    sees: ["ORD-0042", "$980.00", "$1,250.50"],
  },
  {
    op: "invoices_get",
    reply: {
      data: {
        id: B, invoice_nr: "2026-0042", status: "partially_paid", due_date: "2026-12-01", customer_ref: { reference: "PO-77" },
        money: { currency: "USD", decimals: 2, total: 125050, paid: 50000, open: 75050, subtotal: 107800, vat: 17250 },
        items: [{ title: "Wild forest honey 500 g", quantity: "10.00", total_minor: 113680 }, { title: "Beeswax candle, large", quantity: "2.00", total_minor: 11370 }],
      },
    },
    sees: ["2026-0042", "$750.50", "$1,136.80"],
  },
  {
    op: "products_get",
    reply: {
      data: {
        id: A, name: "Wild forest honey 500 g", product_nr: "HON-500", status: "enabled", ean13: "8712345678901",
        price_incl: "11.6600", currency: "USD", decimals: 2, total_available_stock: 20, total_reserved_stock: 4,
        stocks: [
          { id: A, stock_lvl: "18.00", reserved: "4.00", warehouse: { name: "Main store" }, bin: { bin_name: "Box 4" } },
          { id: B, stock_lvl: "6.00", reserved: "0.00", warehouse: { name: "Arusha shop" }, bin: null },
        ],
      },
    },
    sees: ["Wild forest honey 500 g", "$11.66", "Main store › Box 4"],
  },
  {
    op: "till_sales_list",
    reply: {
      data: [
        { order_id: A, number: "ORD-0099", sold_at: "2026-11-12T11:05:00Z", method: "cash", total: 125050, price_mismatch: 0, currency: "TZS", decimals: 0 },
        { order_id: B, number: "ORD-0100", sold_at: "2026-11-12T11:12:00Z", method: "card", total: 48000, price_mismatch: -500, currency: "TZS", decimals: 0 },
      ],
      meta: { total: 42 },
    },
    sees: ["ORD-0099", /TZS\s125,050/, "charged other prices"],
  },
  {
    op: "till_shifts_list",
    reply: {
      data: [
        { id: A, opened_at: "2026-11-12T06:00:00Z", closed_at: null, opening: 50000, expected: 312000, counted: null, difference: null, currency: "TZS", decimals: 0 },
        { id: B, opened_at: "2026-11-11T06:00:00Z", closed_at: "2026-11-11T18:00:00Z", opening: 50000, expected: 290000, counted: 289500, difference: -500, currency: "TZS", decimals: 0 },
      ],
      meta: { total: 2 },
    },
    sees: ["Open", "Short", /TZS\s312,000/, /TZS\s289,500/],
  },
]

/** Stands in for Office's server: the reply's picture, drawn as asked (redraws are unit-tested in Office). */
async function office(page: Page) {
  await page.route("**/api/voice/tools", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>
    const read = READS.find((r) => r.op === body?.operationId)
    if (!read) return route.fallback()
    const view = resultView(read.op, read.reply)!
    await route.fulfill({
      json: {
        status: "completed",
        data: {},
        view: { ...view, ...(body.presentation ? { presentation: body.presentation } : {}) },
        ...(body.replaces ? { replaces: body.replaces } : {}),
      },
    })
  })
}

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

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi())
})

for (const { op, sees } of READS)
  test(`${op}: the answer is shown as a picture, not only spoken`, async ({ page }) => {
    await office(page)
    const agent = await fakeAgent(page)
    await page.goto("/")
    await startText(page)
    ask(agent, op)
    await expect.poll(agent.results).toBe(1)
    // P5: the answer shows inline under the reply.
    const card = page.getByRole("log").first().locator("[data-inline-result]").locator('[data-role="result"]')
    for (const text of sees) await expect(card).toContainText(text)
    const heard = JSON.parse(String(agent.sent.find((e) => e.type === "client_tool_result")?.result)) as Record<string, unknown>
    expect(heard).not.toHaveProperty("view")
    expect(heard.on_screen).toMatchObject({ kind: "list" })
    await expectNoHorizontalScroll(page)
  })

test("stock appended as ranked bars keeps the API's own figures", async ({ page }) => {
  await office(page)
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  ask(agent, "stock_list")
  await expect.poll(agent.results).toBe(1)
  // P5: the answer shows inline under the reply.
  const results = page.getByRole("log").first().locator("[data-inline-result]")
  const sent = () => agent.sent.filter((e) => e.type === "contextual_update" && String(e.text).includes("pictures"))
  await expect.poll(() => sent().length).toBeGreaterThan(0)
  const context = JSON.parse(String(sent().at(-1)!.text)) as { flow_context: { pictures: { id: string; supports: string[] }[] } }
  const picture = context.flow_context.pictures[0]!
  expect(picture.supports).toEqual(["list", "hbar", "bar", "table"])
  ask(agent, "stock_list", { presentation: "hbar", replaces: picture.id })
  await expect.poll(agent.results).toBe(2)
  await expect(results.locator('[data-role="result"]')).toHaveCount(2)
  await expect(results.locator('[data-chart="hbar"]')).toContainText("20")
  await expectNoHorizontalScroll(page)
})
