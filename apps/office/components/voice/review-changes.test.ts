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

  it("nothing the API listed means no summary: old values are never guessed", () => {
    expect(reviewSummary({ changes: { price: 500.5 } })).toBeNull()
    expect(reviewSummary({ record: { id: "x", name: "AFM Stickers" } })).toBeNull()
    expect(reviewSummary({ current: { kind: "product", priced: {} } })).toBeNull()
    expect(reviewSummary({ current: { changes: [{ field: "x" }, "junk"] } })).toBeNull()
  })
})
