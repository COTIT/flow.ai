import { describe, expect, it } from "vitest"
import { recordLinks } from "./links"

const order = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b"
const other = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5c"

describe("recordLinks", () => {
  it("links a created record by its number or name", () => {
    expect(
      recordLinks(
        { write: true, path: "/workspace/w1/orders" },
        { id: order, number: "ORD-0229" }
      )
    ).toEqual([{ label: "ORD-0229", href: `/orders/${order}` }])
    expect(
      recordLinks(
        { write: true, path: "/workspace/w1/products" },
        { data: { id: order, name: "Testing 123" } }
      )
    ).toEqual([{ label: "Testing 123", href: `/products/${order}` }])
  })
  it("links the record in the path when a write changes a part of it", () => {
    expect(
      recordLinks(
        { write: true, path: `/workspace/w1/invoices/${order}/send` },
        { id: other, status: "sent" }
      )
    ).toEqual([{ label: null, href: `/invoices/${order}` }])
  })
  it("gives no link for reads, pages without a detail view, or ids that are not uuids", () => {
    expect(
      recordLinks({ write: false, path: "/workspace/w1/orders" }, { id: order })
    ).toEqual([])
    expect(
      recordLinks(
        { write: true, path: "/workspace/w1/stock/adjustments" },
        { id: order }
      )
    ).toEqual([])
    expect(
      recordLinks({ write: true, path: "/workspace/w1/orders" }, { id: "../x" })
    ).toEqual([])
  })
})
