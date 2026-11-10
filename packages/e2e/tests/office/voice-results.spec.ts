import type { Page } from "@playwright/test"
import { expect, expectFits, test } from "./fake-api"
import officeEn from "../../../../apps/office/messages/en.json" with { type: "json" }
import voiceEn from "../../../../packages/i18n/messages/voice/en.json" with { type: "json" }

// Office hosts the shared voice texts next to its own.
const en = { ...voiceEn, ...officeEn }

const r = en.voice.results
const uuid = (n: number) => `0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a${String(n).padStart(2, "0")}`

/** Text chat with a fake agent; every tool request is answered by `reply(operationId, n)`. */
async function chat(page: Page, reply: (operation: string, n: number) => unknown) {
  const sent: { type?: string }[] = []
  let emit: (event: unknown) => void = () => {}
  const counts = new Map<string, number>()
  await page.route("**/api/voice/session", (route) => {
    const body = route.request().postDataJSON()
    if (route.request().method() === "DELETE") return route.fulfill({ json: { status: "ended" } })
    if (body.command === "heartbeat") return route.fulfill({ json: { status: "active" } })
    return route.fulfill({
      json: {
        sessionId: "results-session",
        conversationId: "results-conversation",
        signedUrl: "wss://api.elevenlabs.io/v1/convai/conversation?conversation_signature=test",
      },
    })
  })
  await page.route("**/api/voice/tools", (route) => {
    const body = route.request().postDataJSON()
    if (body.command !== "execute") return route.fulfill({ json: { status: "completed", data: [] } })
    const n = (counts.get(body.operationId) ?? 0) + 1
    counts.set(body.operationId, n)
    return route.fulfill({ json: reply(body.operationId, n) })
  })
  await page.routeWebSocket("wss://api.elevenlabs.io/**", (socket) => {
    emit = (event) => socket.send(JSON.stringify(event))
    socket.onMessage((raw) => {
      const event = JSON.parse(String(raw))
      sent.push(event)
      if (event.type === "conversation_initiation_client_data")
        emit({
          type: "conversation_initiation_metadata",
          conversation_initiation_metadata_event: {
            conversation_id: "results-conversation",
            agent_output_audio_format: "pcm_16000",
            user_input_audio_format: "pcm_16000",
          },
        })
    })
  })
  await page.goto("/voice")
  await page.getByRole("button", { name: en.voice.textInstead, exact: true }).click()
  await expect(page.getByLabel(en.voice.message)).toBeVisible()
  let id = 0
  return {
    sent,
    call: (operation: string) =>
      emit({
        type: "client_tool_call",
        client_tool_call: {
          tool_name: "execute_genesy_tool",
          tool_call_id: `call-${++id}`,
          parameters: { operation_id: operation, arguments_json: "{}" },
          expects_response: true,
        },
      }),
    results: () => sent.filter((e) => e.type === "client_tool_result").length,
  }
}

const sales = (from: string, to: string, gross: number, orders: number, net: number | null = null) => ({
  status: "completed",
  data: { data: {} },
  view: {
    kind: "sales",
    from,
    to,
    source: "till",
    rows: [{ currency: "TZS", decimals: 0, gross, net, vat: net === null ? null : gross - net, refunded: 0, orders }],
  },
})

test("two sales answers become one comparison chart with a table equivalent", async ({ page, api: _api }, testInfo) => {
  void _api
  await page.emulateMedia({ reducedMotion: "reduce" })
  const voice = await chat(page, (_op, n) =>
    n === 1
      ? sales("2026-11-07T00:00:00+03:00", "2026-11-08T00:00:00+03:00", 1250000, 42, 1059322)
      : sales("2026-11-06T00:00:00+03:00", "2026-11-07T00:00:00+03:00", 980000, 31)
  )
  voice.call("orders_sales_summary")
  await expect.poll(voice.results).toBe(1)
  voice.call("orders_sales_summary")
  await expect.poll(voice.results).toBe(2)
  const log = page.getByRole("log")
  const chart = log.getByRole("region", { name: r.source.till })
  await expect(chart).toHaveCount(1)
  await expect(chart.locator("[data-chart]")).toContainText("TZS 1,250,000")
  await expect(chart.locator("[data-chart]")).toContainText("TZS 980,000")
  await expect(chart.locator("[data-chart]")).toContainText("42 orders")
  // Revenue excl VAT says it is before refunds; a row without the split shows none.
  await expect(chart.locator("[data-chart]")).toContainText("TZS 1,059,322 excl VAT, before refunds")
  await expect(chart.locator("[data-chart]").getByText(/excl VAT/)).toHaveCount(1)
  // The same numbers as a real table, for screen readers and on request.
  const table = chart.getByRole("table")
  await expect(table.getByRole("row")).toHaveCount(3)
  await expect(table.getByRole("columnheader", { name: r.net })).toHaveCount(1)
  await expect(table).toContainText("TZS 190,678")
  await chart.scrollIntoViewIfNeeded()
  await expectFits(page)
  await chart.screenshot({ path: testInfo.outputPath("sales-chart.png") })
  const toggle = chart.getByRole("button", { name: r.showTable })
  await toggle.focus()
  await page.keyboard.press("Enter")
  await expect(chart.getByRole("button", { name: r.showChart })).toHaveAttribute("aria-pressed", "true")
  await expect(table).toBeVisible()
  await expectFits(page)
  await expect(table.getByRole("rowheader").first()).toContainText("Nov 7, 2026")
  await expect(chart.locator("[data-chart]")).toHaveCount(0)
  // The picture is for the screen: the agent's tool result carries no view.
  expect(JSON.stringify(voice.sent)).not.toContain('"view"')
})

test("a list answer shows five record cards linking to Office, the total and the list page", async ({ page, api: _api }, testInfo) => {
  void _api
  const rows = Array.from({ length: 5 }, (_, i) => ({
    id: uuid(i),
    label: `ORD-02${i}`,
    detail: "Neema",
    status: "open",
    money: { minor: 125050, decimals: 2, currency: "USD" },
    stock: null,
    href: `/orders/${uuid(i)}`,
  }))
  const voice = await chat(page, (op) =>
    op === "orders_list"
      ? { status: "completed", data: {}, view: { kind: "records", resource: "orders", total: 23, href: "/orders", rows } }
      : { status: "completed", data: {}, view: { kind: "records", resource: "invoices", total: 0, href: "/invoices", rows: [] } }
  )
  voice.call("orders_list")
  const list = page.getByRole("log").getByRole("region", { name: r.resources.orders })
  await expect(list.getByRole("listitem")).toHaveCount(5)
  const first = list.getByRole("link", { name: /ORD-020/ })
  await expect(first).toHaveAttribute("href", `/orders/${uuid(0)}`)
  await expect(first).toHaveAttribute("target", "_blank")
  await expect(first).toContainText("$1,250.50")
  await expect(first).toContainText("Neema · Open")
  expect((await first.boundingBox())!.height).toBeGreaterThanOrEqual(44)
  await expect(list).toContainText("Showing 5 of 23")
  await expect(list.getByRole("link", { name: r.openAll })).toHaveAttribute("href", "/orders")
  await list.scrollIntoViewIfNeeded()
  await expectFits(page)
  await list.screenshot({ path: testInfo.outputPath("record-list.png") })
  voice.call("invoices_list")
  await expect(page.getByRole("log").getByText(r.empty.invoices)).toBeVisible()
})

test("a lot answer shows where it comes from and its history", async ({ page, api: _api }, testInfo) => {
  void _api
  const voice = await chat(page, (op) =>
    op === "lots_passport_get"
      ? {
          status: "completed",
          data: {},
          view: {
            kind: "lot_origin",
            title: "Ilula forest honey",
            origin: { country: "TZ", region: "Iringa", place: "Ilula" },
            materials: [{ name: "Honey", origin: "Ilula", share: 10000 }],
          },
        }
      : {
          status: "completed",
          data: {},
          view: {
            kind: "trail",
            events: [
              { kind: "status_changed", from: "quarantine", to: "released", at: "2026-11-05T08:00:00Z", by: "Neema" },
              { kind: "created", from: null, to: null, at: "2026-11-01T08:00:00Z", by: null },
            ],
          },
        }
  )
  voice.call("lots_passport_get")
  await expect.poll(voice.results).toBe(1)
  voice.call("lots_events")
  await expect.poll(voice.results).toBe(2)
  const log = page.getByRole("log")
  const origin = log.getByRole("region", { name: r.originTitle })
  await expect(origin).toContainText("Ilula, Iringa, Tanzania")
  await expect(origin).toContainText("Honey")
  await expect(origin).toContainText("100%")
  const trail = log.getByRole("region", { name: r.trailTitle })
  await expect(trail.getByRole("listitem")).toHaveCount(2)
  await expect(trail.getByRole("listitem").first()).toContainText(
    `${r.events.status_changed}: ${en.lots.statuses.quarantine} → ${en.lots.statuses.released}`
  )
  await expect(trail.getByRole("listitem").first()).toContainText("by Neema")
  await trail.scrollIntoViewIfNeeded()
  await expectFits(page)
  await log.screenshot({ path: testInfo.outputPath("lot-origin.png") })
})

test("the attention answer groups what needs dealing with and hides sections the user can't see", async ({ page, api: _api }, testInfo) => {
  void _api
  const item = { detail: null, status: null, money: null, level: null, at: null, attempts: null, href: null }
  const voice = await chat(page, () => ({
    status: "completed",
    data: {},
    view: {
      kind: "attention",
      sections: [
        { key: "orders", counts: { open: 9 }, items: [{ ...item, label: "ORD-0231", status: "open", at: "2026-11-01T08:00:00Z", money: { minor: 1000, decimals: 2, currency: "USD" }, href: `/orders/${uuid(1)}` }] },
        { key: "stock", counts: { negative: 1, out_of_stock: 2 }, items: [{ ...item, label: "AFM Stickers", detail: "Main", level: "0.00", href: `/products/${uuid(2)}` }] },
        { key: "webhooks", counts: { failed: 1 }, items: [{ ...item, label: "order.paid", detail: "timeout", attempts: 5 }] },
        { key: "lots", counts: { quarantined: 0, holds: 0 }, items: [] },
      ],
    },
  }))
  voice.call("attention_summary")
  const card = page.getByRole("log").getByRole("region", { name: r.attentionTitle })
  await expect(card.getByRole("region")).toHaveCount(3)
  await expect(card.getByRole("region", { name: r.sections.orders })).toContainText("Open 9")
  await expect(card.getByRole("link", { name: /ORD-0231/ })).toHaveAttribute("href", `/orders/${uuid(1)}`)
  const stock = card.getByRole("region", { name: r.sections.stock })
  await expect(stock).toContainText(`${r.counts.negative} 1`)
  await expect(stock).toContainText(`${r.counts.out_of_stock} 2`)
  await expect(stock).toContainText("Level 0.00")
  await expect(card.getByRole("region", { name: r.sections.webhooks })).toContainText("5 attempts")
  await expect(card.getByRole("region", { name: r.sections.lots })).toHaveCount(0)
  await card.scrollIntoViewIfNeeded()
  await expectFits(page)
  await card.screenshot({ path: testInfo.outputPath("attention.png") })
})

test("a write review leads with before → after and folds the rest under More details", async ({ page, api: _api }, testInfo) => {
  void _api
  await page.emulateMedia({ reducedMotion: "reduce" })
  const product = uuid(3)
  const voice = await chat(page, () => ({
    status: "requires_confirmation",
    data: { actionId: "a-price" },
    review: {
      operationId: "products_update",
      method: "PATCH",
      ticket: "price-ticket",
      confirmationCode: "531842",
      confirmationPhrase: "approve five three one",
      summary: "Update product",
      target: "products",
      details: {
        workspace: "w1",
        target: { product },
        record: { id: product, name: "AFM Stickers" },
        current: {
          kind: "product",
          changes: [
            { field: "price", currency: "USD", before: "$0.50", after: "$500.50", change_bp: 10000000, warning: "large_change" },
            { field: "purchase_price", currency: "USD", before: "0.005", after: "0.0075" },
          ],
        },
        changes: { price: "500.50" },
      },
      expiresAt: new Date(Date.now() + 120000).toISOString(),
    },
  }))
  voice.call("products_update")
  const diff = page.locator("[data-review-diff]")
  await expect(diff).toBeVisible()
  // Sentence case for every label, labelled or not.
  await expect(diff.getByRole("term")).toHaveText(["Price", en.voice.reviewFields.purchase_price])
  await expect(diff.locator("del").first()).toHaveText("was $0.50")
  await expect(diff.locator("ins").first()).toHaveText("now $500.50")
  await expect(diff).toContainText("Up 100,000%")
  // Sub-cent prices stay exact, as the API gave them.
  await expect(diff.locator("ins").nth(1)).toHaveText("now 0.0075 USD")
  await expect(page.getByText(r.largeChange)).toBeVisible()
  const more = page.getByRole("button", { name: r.moreDetails })
  await expect(more).toHaveAttribute("aria-expanded", "false")
  await expect(page.getByText(/Field:/)).toHaveCount(0)
  await expectFits(page)
  await page.screenshot({ path: testInfo.outputPath("review-diff.png") })
  await more.focus()
  await page.keyboard.press("Enter")
  await expect(page.getByRole("button", { name: r.fewerDetails })).toHaveAttribute("aria-expanded", "true")
  await expect(page.getByText(en.voice.reviewFields.current, { exact: true })).toBeVisible()
  await expect(page.getByText(/Field:/)).toHaveCount(0)
  await expectFits(page)
})

test("a delete review names the record, warns, and shows what else it touches", async ({ page, api: _api }, testInfo) => {
  void _api
  await page.emulateMedia({ reducedMotion: "reduce" })
  const voice = await chat(page, () => ({
    status: "requires_confirmation",
    data: { actionId: "a-archive" },
    review: {
      operationId: "relations_delete",
      method: "DELETE",
      ticket: "archive-ticket",
      confirmationCode: "220917",
      confirmationPhrase: "approve two two zero",
      summary: "Archive customer",
      target: "relations",
      details: {
        workspace: "w1",
        record: { type: "customers", label: null },
        warnings: ["This customer has 3 open orders."],
        impact: { effect: "archive", orders: 3, contacts: 2 },
      },
      expiresAt: new Date(Date.now() + 120000).toISOString(),
    },
  }))
  voice.call("relations_delete")
  const diff = page.locator("[data-review-diff]")
  await expect(diff).toContainText(`customer: ${r.nameUnavailable}`)
  await expect(diff).toContainText(r.effect.archive)
  await expect(diff.getByRole("listitem")).toHaveText(["Orders 3", "Contacts 2"])
  await expect(page.getByText("This customer has 3 open orders.")).toBeVisible()
  await expect(page.getByText(/Field:/)).toHaveCount(0)
  await expectFits(page)
  await page.screenshot({ path: testInfo.outputPath("review-delete.png") })
})
