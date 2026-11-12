import { describe, expect, it } from "vitest"
import { reviewSummary } from "./review-changes"

describe("reviewSummary", () => {
  it("reads the changed fields the API listed, with per-change warnings", () => {
    expect(
      reviewSummary({
        target: { product: "p" },
        current: {
          kind: "product",
          changes: [
            { field: "price", currency: "USD", before: "$0.50", after: "$500.50", change_bp: 10000000, warning: "large_change" },
            { field: "cost", currency: "USD", before: "0.005", after: "0.0075" },
            { field: "name", before: null, after: "AFM Stickers" },
            { field: "unit", before: 1, after: 2 },
          ],
        },
      })
    ).toEqual({
      record: null,
      warnings: [],
      impact: [],
      rows: [
        { field: "price", before: "$0.50", after: "$500.50", change: 10000000, warning: true },
        // Sub-cent prices come as exact decimals: shown as given, with the currency code.
        { field: "cost", before: "0.005 USD", after: "0.0075 USD", change: null, warning: false },
        { field: "name", before: null, after: "AFM Stickers", change: null, warning: false },
        { field: "unit", before: "1", after: "2", change: null, warning: false },
      ],
    })
  })

  it("a delete card shows the record, the warnings and what it affects", () => {
    expect(
      reviewSummary({
        record: { type: "contacts", label: null },
        warnings: ["This customer has 3 open orders."],
        impact: { effect: "archive", orders: 3, contacts: 2 },
      })
    ).toEqual({
      record: { type: "contacts", label: null },
      warnings: ["This customer has 3 open orders."],
      impact: [
        { key: "orders", count: 3 },
        { key: "contacts", count: 2 },
      ],
      effect: "archive",
      rows: [],
    })
  })

  it("a stock transfer row keeps the API's warehouse name as its label", () => {
    const summary = reviewSummary({
      current: {
        changes: [
          { field: "stock_from", label: "Main store", before: "120", after: "100" },
          { field: "stock_to", label: "Shop", before: "5", after: "25" },
          { field: "price", label: "  ", after: "2" },
        ],
      },
    })
    expect(summary?.rows.map((row) => [row.label, row.before, row.after])).toEqual([
      ["Main store", "120", "100"],
      ["Shop", "5", "25"],
      [undefined, null, "2"],
    ])
  })

  it("nothing the API listed means no summary: old values are never guessed", () => {
    expect(reviewSummary({ changes: { price: 500.5 } })).toBeNull()
    expect(reviewSummary({ record: { id: "x", name: "AFM Stickers" } })).toBeNull()
    expect(reviewSummary({ current: { kind: "product", priced: {} } })).toBeNull()
    expect(reviewSummary({ current: { changes: [{ field: "x" }, "junk"] } })).toBeNull()
  })

  it("P3-3: a stock review leads with the API's warehouse row, new amount visible", () => {
    const summary = reviewSummary({
      record: { type: "products", label: "Raw Honey 500g" },
      current: { kind: "stock", changes: [{ field: "stock", label: "Main store", before: "14", after: "130" }] },
      changes: { warehouses: [{ warehouse_id: "0190c1f2-0000-7000-8000-000000000001", stock_lvl: "130" }] },
    })
    expect(summary?.rows).toEqual([
      { field: "stock", label: "Main store", before: "14", after: "130", change: null, warning: false },
    ])
  })

  describe("without API rows, the request's own plain values lead (new values only)", () => {
    const record = { type: "relations", label: "Mama Nzuri" }
    it("keeps names, dates, quantities and exact amounts; drops ids, codes and nested data", () => {
      expect(
        reviewSummary({
          record,
          current: { kind: "invoice", currency: "EUR", changes: [] },
          changes: {
            name: "Mama Nzuri Ltd",
            relation_id: "0190c1f2-0000-7000-8000-000000000001",
            id: 7,
            note: "0190c1f2-0000-7000-8000-000000000002",
            payment_method: "bank_transfer",
            amount: "125.50",
            date: "2026-10-23",
            quantity: 3,
            address: { city: "Arusha" },
            items: [{ title: "Chair" }],
            is_credit: false,
            description: "  ",
          },
        })?.rows
      ).toEqual([
        { field: "name", before: null, after: "Mama Nzuri Ltd", change: null, warning: false },
        { field: "amount", before: null, after: "125.50 EUR", change: null, warning: false },
        { field: "date", before: null, after: "2026-10-23", change: null, warning: false },
        { field: "quantity", before: null, after: "3", change: null, warning: false },
      ])
    })
    it("never shows money from a float", () => {
      expect(reviewSummary({ record, changes: { price: 12.5, amount: 3 } })?.rows).toEqual([
        { field: "amount", before: null, after: "3", change: null, warning: false },
      ])
    })
    it("with nothing plain to show, the record line and More details stay; no empty rows", () => {
      expect(reviewSummary({ record, changes: { relation_id: "x", lines: [1] } })).toEqual({
        record,
        warnings: [],
        impact: [],
        rows: [],
      })
    })
    it("without a record line nothing was folded, so the card stays whole (no summary)", () => {
      const current = { locked: { currency: "EUR", refundable: "€12.50" } }
      expect(reviewSummary({ current, changes: { amount: "12.50" } })).toBeNull()
    })
  })
})
