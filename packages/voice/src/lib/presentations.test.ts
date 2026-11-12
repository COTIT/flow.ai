import { describe, expect, it } from "vitest"
import { choosePresentation, drawnAs, KINDS, resultView, shapeOf, type ResultView } from "./views"

// The approved shape × kind matrix (plan REVISION 2): allowed kinds, default first.
const MATRIX: Record<string, string[]> = {
  series: ["line", "sparkline", "bar", "table"],
  totals: ["hbar", "bar", "kpi", "table"],
  comparison: ["delta", "bar", "hbar", "kpi", "table"],
  breakdown: ["hbar", "bar", "pie", "donut", "table", "list"],
  share: ["list", "bar", "hbar", "pie", "donut", "meter", "table"],
  counts: ["list", "bar", "hbar", "pie", "donut", "kpi", "table"],
  rows: ["list", "kpi", "table"],
  events: ["timeline", "list", "table"],
  // P2: stock per item or bin (no API single figure; units may differ, so no pie), search hits,
  // and one record with its lines (no chart of one record).
  quantities: ["list", "hbar", "bar", "table"],
  hits: ["list", "table"],
  document: ["list", "table", "kpi"],
}

const day = (date: string, gross: number) => ({ date, gross, net: gross, orders: 1 })
const VIEWS: Record<string, ResultView> = {
  series: { kind: "sales_daily", from: "2026-11-01", to: "2026-11-03", time_zone: "UTC", currencies: [{ currency: "USD", decimals: 2, days: [day("2026-11-01", 5), day("2026-11-02", 9)] }] },
  totals: { kind: "sales", from: "2026-11-01", to: "2026-11-08", source: null, rows: [{ currency: "USD", decimals: 2, gross: 9, net: 8, vat: 1, refunded: 0, orders: 2 }] },
  comparison: { kind: "comparison", from: "2026-11-01", to: "2026-11-08", base_from: "2026-10-25", base_to: "2026-11-01", currencies: [{ currency: "USD", decimals: 2, current: 900, base: 600, change_bp: 5000 }] },
  breakdown: { kind: "breakdown", by: "products", from: "2026-11-01", to: "2026-11-08", currencies: [{ currency: "USD", decimals: 2, rows: [{ label: "Honey", gross: 500, orders: 2 }] }] },
  share: { kind: "lot_origin", title: null, origin: null, materials: [{ name: "Honey", origin: null, share: 10000 }] },
  counts: { kind: "attention", sections: [{ key: "orders", counts: { pending: 3 }, items: [] }] },
  rows: { kind: "records", resource: "orders", total: 9, href: "/orders", rows: [] },
  events: { kind: "trail", events: [{ kind: "created", from: null, to: null, at: "2026-11-01", by: null }] },
  quantities: { kind: "stock", scope: "list", total: 1, href: "/stock", groups: [{ title: "Main", rows: [{ label: "Honey", detail: null, available: "9.00", level: "12.00", reserved: "3.00", minimum: 5, href: null }] }] },
  hits: { kind: "search", groups: [{ key: "products", rows: [{ label: "Honey", detail: null, status: null, money: null, href: null }] }] },
  document: { kind: "document", resource: "orders", title: "ORD-7", status: "open", href: "/orders/x", facts: [], lineMoney: "excl", lines: [], totals: [{ key: "total", money: { minor: 5, decimals: 2, currency: "USD" } }] },
}

describe("every chart kind against every data shape", () => {
  it("knows all thirteen kinds", () => {
    expect([...KINDS].sort()).toEqual(
      ["bar", "delta", "donut", "hbar", "kpi", "line", "list", "map", "meter", "pie", "sparkline", "table", "timeline"]
    )
  })

  for (const [shape, allowed] of Object.entries(MATRIX)) {
    const view = VIEWS[shape]!
    it(`${shape}: classified by its data and drawn as ${allowed[0]} by default`, () => {
      expect(shapeOf(view)).toBe(shape)
      expect(drawnAs(view)).toBe(allowed[0])
    })
    for (const kind of KINDS) {
      const ok = allowed.includes(kind)
      it(`${shape} × ${kind} → ${ok ? "allowed" : "refused"}`, () => {
        const { view: shown, note } = choosePresentation(view, kind)
        if (ok) {
          expect(note).toBeUndefined()
          expect(drawnAs(shown)).toBe(kind)
        } else {
          // Never faked to fit: the default stays, with why and the closest fit it supports.
          expect(drawnAs(shown)).toBe(allowed[0])
          expect(note).toMatchObject({ asked: kind, shown: allowed[0] })
          expect(allowed).toContain(note!.closest)
          expect(note!.reason).toMatch(/\w{3,}/)
        }
      })
    }
  }

  it("never echoes a word outside the vocabulary", () => {
    expect(choosePresentation(VIEWS.series!, "<b>3d</b>").note).toMatchObject({ asked: "unknown", shown: "line" })
  })
})

describe("new pictures from the API", () => {
  it("a period comparison keeps both periods and the API's change, per currency", () => {
    const view = resultView("orders_sales_compare", {
      data: {
        period: { from: "2026-11-01T00:00:00Z", to: "2026-11-08T00:00:00Z" },
        compare_to_period: { from: "2026-10-25T00:00:00Z", to: "2026-11-01T00:00:00Z" },
        time_zone: "UTC",
        currencies: [
          { currency: "TZS", decimals: 0, current: { gross_total: 900, orders: 3 }, base: { gross_total: 600, orders: 2 }, gross_change_bp: 5000 },
          { currency: "USD", decimals: 2, current: { gross_total: 5, orders: 1 }, base: { gross_total: 0, orders: 0 } },
        ],
      },
    })
    expect(view).toEqual({
      kind: "comparison",
      from: "2026-11-01T00:00:00Z",
      to: "2026-11-08T00:00:00Z",
      base_from: "2026-10-25T00:00:00Z",
      base_to: "2026-11-01T00:00:00Z",
      currencies: [
        { currency: "TZS", decimals: 0, current: 900, base: 600, change_bp: 5000 },
        { currency: "USD", decimals: 2, current: 5, base: 0, change_bp: null },
      ],
    })
    expect(resultView("orders_sales_compare", { data: { currencies: "x" } })).toBeUndefined()
  })

  it("top customers are ranked as the API ranked them, walk-ins apart, nothing summed", () => {
    const id = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b"
    expect(
      resultView("orders_top_customers", {
        data: {
          from: "2026-11-01T00:00:00Z",
          to: "2026-11-08T00:00:00Z",
          currencies: [
            {
              currency: "TZS",
              decimals: 0,
              rows: [
                { relation: { id, name: "Neema Shop" }, orders: 4, gross_total: 900000 },
                { relation: { id }, orders: 1, gross_total: 5 },
              ],
              without_customer: { orders: 9, gross_total: 300000 },
            },
          ],
        },
      })
    ).toEqual({
      kind: "breakdown",
      by: "customers",
      from: "2026-11-01T00:00:00Z",
      to: "2026-11-08T00:00:00Z",
      currencies: [{ currency: "TZS", decimals: 0, rows: [{ label: "Neema Shop", gross: 900000, orders: 4 }, { label: null, gross: 300000, orders: 9 }] }],
    })
  })

  it("top products by their sold line title", () => {
    const view = resultView("products_top_sellers", {
      data: { from: "a", to: "b", currencies: [{ currency: "USD", decimals: 2, rows: [{ product: { name: "Honey" }, title: "Honey 500 g", orders: 3, gross_total: 4500 }] }] },
    })
    expect(view).toMatchObject({ kind: "breakdown", by: "products", currencies: [{ rows: [{ label: "Honey 500 g", gross: 4500, orders: 3 }] }] })
  })
})
