import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vitest"

import type { ResultView } from "../../lib/views"
import { VoiceContext } from "../provider"
import { stubVoice } from "../stub-voice"
import { VoiceResult } from "./index"

vi.mock("next-intl", () => ({
  useTranslations: () => Object.assign((key: string) => key, { has: () => false }),
  useLocale: () => "en",
}))

const orders: ResultView[] = [
  {
    kind: "records",
    resource: "orders",
    total: 5,
    href: "/orders?status=open",
    rows: [
      { id: "1", label: "Order 1", detail: null, status: "open", money: null, stock: null, href: "/orders/1" },
    ],
  } as ResultView,
]

const html = (node: React.ReactNode, resolveHref?: (href: string) => string) =>
  renderToStaticMarkup(
    <VoiceContext.Provider value={stubVoice(resolveHref ? { resolveHref } : {})}>{node}</VoiceContext.Provider>
  )

describe("VoiceResult links", () => {
  it("keeps server hrefs as they are by default", () => {
    const out = html(<VoiceResult views={orders} />)
    expect(out).toContain('href="/orders/1"')
    expect(out).toContain('href="/orders?status=open"')
  })

  it("passes every server href through the host's resolveHref", () => {
    const out = html(<VoiceResult views={orders} />, (href) => `https://office.test${href}`)
    expect(out).toContain('href="https://office.test/orders/1"')
    expect(out).toContain('href="https://office.test/orders?status=open"')
  })

  it("draws no link where the host refuses the href", () => {
    const out = html(<VoiceResult views={orders} />, () => "")
    expect(out).not.toContain("<a")
    expect(out).toContain("Order 1")
  })
})

describe("VoiceResult status labels", () => {
  it("falls back to the humanized status without a host label", () => {
    expect(html(<VoiceResult views={orders} />)).toContain("Open")
  })

  it("uses the host's statusLabel when given", () => {
    const out = html(
      <VoiceResult views={orders} statusLabel={(domain, status) => `${domain}:${status}`} />
    )
    expect(out).toContain("orders:open")
  })
})

describe("VoiceResult daily sales", () => {
  it("draws a trend line over the days (never a progress bar per day), keeping the API's numbers", () => {
    const daily = {
      kind: "sales_daily",
      from: "2026-11-09",
      to: "2026-11-10",
      time_zone: "UTC",
      currencies: [
        {
          currency: "USD",
          decimals: 2,
          days: [
            { date: "2026-11-09", gross: 1000, net: 1000, orders: 2 },
            { date: "2026-11-10", gross: 0, net: 0, orders: 0 },
          ],
        },
      ],
    } as ResultView
    const out = html(<VoiceResult views={[daily]} />)
    expect(out).toContain('data-kind="sales_daily"')
    expect(out).toContain('data-chart="line"')
    expect(out.match(/<polyline/g)).toHaveLength(1)
    expect(out).not.toContain("<rect")
    expect(out).toContain("$10.00")
    expect(out).not.toContain("noSales")
  })
})

describe("VoiceResult draws every allowed kind from the API's own figures", () => {
  const daily = {
    kind: "sales_daily", from: "2026-11-09", to: "2026-11-11", time_zone: "UTC",
    currencies: [{ currency: "USD", decimals: 2, days: [
      { date: "2026-11-09", gross: 1000, net: 1000, orders: 2 },
      { date: "2026-11-10", gross: 500, net: 500, orders: 1 },
    ] }],
  }
  const breakdown = {
    kind: "breakdown", by: "customers", from: "2026-11-01", to: "2026-11-08",
    currencies: [{ currency: "USD", decimals: 2, rows: [{ label: "Neema Shop", gross: 900000, orders: 4 }, { label: null, gross: 300000, orders: 9 }] }],
  }
  const comparison = {
    kind: "comparison", from: "2026-11-01", to: "2026-11-08", base_from: "2026-10-25", base_to: "2026-11-01",
    currencies: [{ currency: "USD", decimals: 2, current: 90000, base: 60000, change_bp: 5000 }],
  }
  const origin = { kind: "lot_origin", title: "Lot 7", origin: null, materials: [{ name: "Honey", origin: null, share: 7500 }, { name: "Wax", origin: null, share: 2500 }] }
  const counts = { kind: "attention", sections: [{ key: "orders", counts: { open: 3, acknowledged: 1 }, items: [] }] }
  const rows = { kind: "records", resource: "orders", total: 42, href: "/orders", rows: [] }
  const trail = { kind: "trail", events: [{ kind: "created", from: null, to: null, at: "2026-11-01", by: null }] }
  const as = (view: object, presentation?: string) => html(<VoiceResult views={[{ ...view, ...(presentation ? { presentation } : {}) } as ResultView]} />)

  it.each([
    ["line", daily, "<polyline"],
    ["sparkline", daily, "h-10"],
    ["bar", daily, "rounded-t"],
    ["hbar", breakdown, "$9,000.00"],
    ["table", daily, '<table id='],
    ["list", breakdown, "breakdown.walkIn"],
    ["kpi", rows, ">42<"],
    ["meter", origin, "75%"],
  ])("%s", (kind, view, expected) => {
    const out = as(view, kind === "hbar" ? undefined : kind)
    expect(out).toContain(expected)
    if (kind !== "table" && kind !== "list") expect(out).toContain(`data-chart="${kind}"`)
  })

  it("pie and donut: one slice per API amount, named by the API's labels and amounts, no share printed", () => {
    for (const kind of ["pie", "donut"]) {
      const out = as(breakdown, kind)
      expect(out).toContain(`data-chart="${kind}"`)
      expect(out.match(/<path/g)).toHaveLength(2)
      expect(out).toContain('aria-label="Neema Shop: $9,000.00; breakdown.walkIn: $3,000.00"')
      expect(out).not.toContain("%")
    }
    expect(as(counts, "pie").match(/<path/g)).toHaveLength(2)
  })

  it("a comparison shows the current period and the API's change, as a delta by default", () => {
    const out = as(comparison)
    expect(out).toContain('data-chart="delta"')
    expect(out).toContain("$900.00")
    expect(out).toContain("$600.00")
    expect(out).toContain("compare.change")
    // As a KPI tile: the current period only, the base is never folded in.
    expect(as(comparison, "kpi")).not.toContain("$600.00")
  })

  it("an events table lists each event with its date", () => {
    expect(as(trail, "table")).toContain("Created")
  })
})

describe("P2 pictures", () => {
  const A = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b"
  const stock = {
    kind: "stock", scope: "list", total: 31, href: "/stock",
    groups: [
      { title: "Main store", rows: [{ label: "Honey 500 g", detail: "H-500 · L-7", available: "9.00", level: "12.00", reserved: "3.00", minimum: 5, href: `/products/${A}` }] },
      { title: "Shop", rows: [{ label: "Wax", detail: null, available: "-1.00", level: "-1.00", reserved: "0.00", minimum: null, href: null }] },
    ],
  } as ResultView
  const bins = {
    kind: "stock", scope: "bins", total: null, href: null,
    groups: [{ title: "Main store", rows: [{ label: null, detail: "L-9", available: null, level: "2.00", reserved: "0.00", minimum: null, href: null }] }],
  } as ResultView
  const search = {
    kind: "search",
    groups: [
      { key: "orders", rows: [{ label: "ORD-7", detail: null, status: "open", money: { minor: 125050, decimals: 2, currency: "USD" }, href: `/orders/${A}` }] },
      { key: "relations", rows: [{ label: "Neema Shop", detail: null, status: null, money: null, href: null }] },
      { key: "lots", rows: [] },
    ],
  } as ResultView
  const order = {
    kind: "document", resource: "orders", title: "ORD-7", status: "open", href: `/orders/${A}`,
    facts: [{ key: "customer", text: "Neema Shop" }, { key: "created", date: "2026-11-10" }],
    lineMoney: "excl",
    lines: [{ label: "Honey 500 g", quantity: "2.00", money: { minor: 2000, decimals: 2, currency: "USD" } }],
    totals: [{ key: "vat", money: { minor: 360, decimals: 2, currency: "USD" } }, { key: "total", money: { minor: 2360, decimals: 2, currency: "USD" } }],
  } as ResultView
  const draft = { ...order, resource: "invoices", title: null, status: "draft", lineMoney: "incl" } as ResultView
  const product = {
    kind: "document", resource: "products", title: "Honey 500 g", status: "enabled", href: `/products/${A}`,
    facts: [{ key: "priceIncl", text: "12.50" }, { key: "available", text: "9" }],
    lineMoney: null, lines: [{ label: "Main store › Box 4", quantity: "12.00", money: null }], totals: [],
  } as ResultView

  it("stock: one section per warehouse, available as the API wrote it, each product linked", () => {
    const out = html(<VoiceResult views={[stock]} />)
    expect(out).toContain('data-kind="stock"')
    expect(out).toContain('aria-label="Main store"')
    expect(out).toContain('aria-label="Shop"')
    expect(out).toContain(`href="/products/${A}"`)
    // Quantities read as counts: whole numbers without decimals, like the product card.
    expect(out).toContain(">9<")
    expect(out).toContain(">-1<")
    expect(out).not.toContain("9.00")
    expect(out).toContain("results.stock.reserved")
    expect(out).toContain('href="/stock"')
  })

  it("stock per bin: the level, and a plain word for stock without a bin", () => {
    const out = html(<VoiceResult views={[bins]} />)
    expect(out).toContain(">2<")
    expect(out).toContain("results.stock.noBin")
    expect(out).not.toContain("<a")
  })

  it("search: a group per kind with its hits; a customer is named, never linked; empty groups are left out", () => {
    const out = html(<VoiceResult views={[search]} />)
    expect(out).toContain("results.search.orders")
    expect(out).toContain(`href="/orders/${A}"`)
    expect(out).toContain("$1,250.50")
    expect(out).toContain("Neema Shop")
    expect(out.match(/<a /g)).toHaveLength(1)
    expect(out).not.toContain("results.search.lots")
  })

  it("an order: headline, facts, lines and the API's totals; open in Office", () => {
    const out = html(<VoiceResult views={[order]} />)
    expect(out).toContain("ORD-7")
    expect(out).toContain("Neema Shop")
    expect(out).toContain("results.facts.customer")
    expect(out).toContain("$20.00")
    expect(out).toContain("$3.60")
    expect(out).toContain("$23.60")
    expect(out).toContain("results.document.linesExcl")
    expect(out).toContain(`href="/orders/${A}"`)
  })

  it("a draft invoice has no number yet and says so", () => {
    expect(html(<VoiceResult views={[draft]} />)).toContain("results.document.draft")
  })

  it("a product priced in its currency shows the API's price with that currency", () => {
    const priced = { ...product, facts: [{ key: "priceIncl", price: "11.6600", currency: "EUR", decimals: 2 }] } as ResultView
    expect(html(<VoiceResult views={[priced]} />)).toContain("€11.66")
  })

  it("a product: price as the API's text and its stock per warehouse", () => {
    const out = html(<VoiceResult views={[product]} />)
    expect(out).toContain("12.50")
    expect(out).toContain("Main store › Box 4")
    expect(out).toContain(">12<")
    expect(out).toContain("results.document.stock")
  })
})

describe("P2 pictures redrawn as another allowed kind", () => {
  const stock = {
    kind: "stock", scope: "list", total: 2, href: "/stock",
    groups: [{ title: "Main store", rows: [
      { label: "Honey", detail: null, available: "9.00", level: "12.00", reserved: "3.00", minimum: null, href: null },
      { label: "Wax", detail: null, available: "-1.00", level: "-1.00", reserved: "0.00", minimum: null, href: null },
    ] }],
  }
  const order = {
    kind: "document", resource: "orders", title: "ORD-7", status: "open", href: "/orders/x", facts: [], lineMoney: "excl", lines: [],
    totals: [{ key: "vat", money: { minor: 360, decimals: 2, currency: "USD" } }, { key: "total", money: { minor: 2360, decimals: 2, currency: "USD" } }],
  }
  const search = { kind: "search", groups: [{ key: "orders", rows: [{ label: "ORD-7", detail: null, status: "open", money: { minor: 5, decimals: 2, currency: "USD" }, href: null }] }] }
  const as = (view: object, presentation: string) => html(<VoiceResult views={[{ ...view, presentation } as ResultView]} />)

  it("stock as ranked bars keeps the API's text and draws a negative level as an empty bar", () => {
    const out = as(stock, "hbar")
    expect(out).toContain('data-chart="hbar"')
    expect(out).toContain(">9<")
    expect(out).toContain(">-1<")
    expect(out.match(/fill-\(--gold\)/g)).toHaveLength(1)
  })

  it("stock and search as tables", () => {
    expect(as(stock, "table")).toContain("Honey")
    expect(as(search, "table")).toContain("$0.05")
  })

  it("an order as a single figure shows its total, never a sum", () => {
    const out = as(order, "kpi")
    expect(out).toContain('data-chart="kpi"')
    expect(out).toContain("$23.60")
    expect(out).not.toContain("$3.60")
  })
})

describe("till pictures", () => {
  const A = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b"
  const tzs = (minor: number) => ({ minor, decimals: 0, currency: "TZS" })
  const sales = {
    kind: "till_sales", total: 42, href: "/till/sales",
    rows: [
      { label: "ORD-0099", at: "2026-11-12T11:05:00Z", method: "cash", mismatch: false, money: tzs(125050), href: `/orders/${A}` },
      { label: "ORD-0100", at: "2026-11-12T11:07:00Z", method: "card", mismatch: true, money: null, href: null },
    ],
  } as ResultView
  const shifts = {
    kind: "till_shifts", total: 2, href: "/till/shifts",
    rows: [
      { opened: "2026-11-12T06:00:00Z", closed: null, state: "open", expected: tzs(312000), counted: null, difference: null },
      { opened: "2026-11-11T06:00:00Z", closed: "2026-11-11T18:00:00Z", state: "short", expected: tzs(290000), counted: tzs(289500), difference: tzs(-500) },
    ],
  } as ResultView

  it("sales: each sale linked to its order, its total when known, a plain note when the till charged other prices", () => {
    const out = html(<VoiceResult views={[sales]} />)
    expect(out).toContain("ORD-0099")
    expect(out).toMatch(/TZS\s125,050/)
    expect(out).toContain(`href="/orders/${A}"`)
    expect(out).toContain("results.till.mismatch")
    expect(out).toContain("Card")
    expect(out).toContain("42")
  })

  it("shifts: open with the cash expected so far; closed with counted and short by the API's difference", () => {
    const out = html(<VoiceResult views={[shifts]} />)
    expect(out).toContain("results.till.state.open")
    expect(out).toContain("results.till.state.short")
    expect(out).toMatch(/TZS\s312,000/)
    expect(out).toMatch(/TZS\s289,500/)
    expect(out).toMatch(/-TZS\s500|TZS\s-500/)
  })

  it("without amounts a shift still says open, short, over or even", () => {
    const bare = { ...shifts, rows: [{ ...(shifts as { rows: object[] }).rows[1], expected: null, counted: null, difference: null }] } as ResultView
    const out = html(<VoiceResult views={[bare]} />)
    expect(out).toContain("results.till.state.short")
    expect(out).not.toContain("TZS")
  })
})
