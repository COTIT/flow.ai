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
