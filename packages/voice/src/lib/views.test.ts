import { describe, expect, it } from "vitest"
import { resultView } from "./views"

const a = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b"
const b = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5c"
const order = (id: string, number: string) => ({
  id,
  number,
  status: "open",
  total: 125050,
  decimals: 2,
  currency: "USD",
  customer: { name: "Neema" },
})
const page = (data: unknown[], total = data.length) => ({
  data,
  meta: { total, current_page: 1, last_page: 1, per_page: 25, from: 1 },
})

describe("resultView", () => {
  it("shows nothing for operations outside the allow-list", () => {
    expect(resultView("orders_create", { data: order(a, "ORD-1") })).toBeUndefined()
    expect(resultView("relations_list", page([]))).toBeUndefined()
  })

  it("keeps sales per currency exactly as the API returned them", () => {
    const view = resultView("orders_sales_summary", {
      data: {
        from: "2026-11-07T00:00:00+03:00",
        to: "2026-11-08T00:00:00+03:00",
        source: "till",
        currencies: [
          {
            currency: "TZS", decimals: 0, gross_total: 1250000, net_total: 1059322,
            vat_total: 190678, refunded_total: 0, orders: 42,
          },
          // An older API without the VAT split still renders.
          { currency: "USD", decimals: 2, gross_total: 9900, refunded_total: 100, orders: 3 },
        ],
      },
    })
    expect(view).toEqual({
      kind: "sales",
      from: "2026-11-07T00:00:00+03:00",
      to: "2026-11-08T00:00:00+03:00",
      source: "till",
      rows: [
        { currency: "TZS", decimals: 0, gross: 1250000, net: 1059322, vat: 190678, refunded: 0, orders: 42 },
        { currency: "USD", decimals: 2, gross: 9900, net: null, vat: null, refunded: 100, orders: 3 },
      ],
    })
  })

  it("lists at most five records with links, the total and the list page", () => {
    const rows = Array.from({ length: 7 }, (_, i) =>
      order(`0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5${i}`, `ORD-${i}`)
    )
    const view = resultView("orders_list", page(rows, 23))
    expect(view).toMatchObject({ kind: "records", resource: "orders", total: 23, href: "/orders" })
    expect(view?.kind === "records" && view.rows).toHaveLength(5)
    expect(view?.kind === "records" && view.rows[0]).toEqual({
      id: "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a50",
      label: "ORD-0",
      detail: "Neema",
      status: "open",
      money: { minor: 125050, decimals: 2, currency: "USD" },
      stock: null,
      href: "/orders/0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a50",
    })
  })

  it("an empty list is still a view, so the screen can say so", () => {
    expect(resultView("invoices_list", page([], 0))).toEqual({
      kind: "records",
      resource: "invoices",
      total: 0,
      href: "/invoices",
      rows: [],
    })
  })

  it("never links a record whose id is not a uuid and caps long text", () => {
    const view = resultView("products_list", page([
      { id: "../settings", name: "Bad", status: "active", total_available_stock: 0 },
      { id: b, name: "x".repeat(300), product_nr: "P-1", status: "active", total_available_stock: 12.5 },
    ]))
    if (view?.kind !== "records") throw new Error("expected records")
    expect(view.rows).toHaveLength(1)
    expect(view.rows[0]!.label).toHaveLength(120)
    expect(view.rows[0]).toMatchObject({ detail: "P-1", stock: 12.5, money: null, href: `/products/${b}` })
  })

  it("reads invoice numbers, customers and open amounts", () => {
    const view = resultView("invoices_list", page([{
      id: a,
      invoice_nr: "INV-7",
      status: "sent",
      customer_name: "Acme",
      money: { currency: "EUR", decimals: 2, open: 5000, paid: 0, subtotal: 5000, total: 5000, vat: 0 },
    }]))
    expect(view?.kind === "records" && view.rows[0]).toMatchObject({
      label: "INV-7",
      detail: "Acme",
      status: "sent",
      money: { minor: 5000, decimals: 2, currency: "EUR" },
    })
  })

  it("lot origin comes from the published passport, else the draft", () => {
    const passport = (title: string) => ({
      title,
      origin: { country: "TZ", region: "Iringa", place: "Ilula" },
      materials: [{ name: "Honey", origin: "Ilula", share_bp: 10000 }],
    })
    expect(resultView("lots_passport_get", {
      data: { draft: passport("Draft"), published: passport("Live"), version: 2, published_at: null },
    })).toEqual({
      kind: "lot_origin",
      title: "Live",
      origin: { country: "TZ", region: "Iringa", place: "Ilula" },
      materials: [{ name: "Honey", origin: "Ilula", share: 10000 }],
    })
    expect(resultView("lots_passport_get", {
      data: { draft: { title: null, origin: null, materials: [] }, published: null, version: 0, published_at: null },
    })).toEqual({ kind: "lot_origin", title: null, origin: null, materials: [] })
  })

  it("a lot's trail keeps the newest six changes and who made them", () => {
    const events = Array.from({ length: 8 }, (_, i) => ({
      id: `e${i}`,
      kind: "status_changed",
      from_status: "quarantine",
      to_status: "released",
      created_at: `2026-11-0${i + 1}T08:00:00Z`,
      user: { id: a, name: i ? "Neema" : null },
      meta: null,
    }))
    const view = resultView("lots_events", page(events))
    if (view?.kind !== "trail") throw new Error("expected trail")
    expect(view.events).toHaveLength(6)
    expect(view.events[0]).toEqual({
      kind: "status_changed",
      from: "quarantine",
      to: "released",
      at: "2026-11-01T08:00:00Z",
      by: null,
    })
  })

  it("attention hides sections the caller may not see and keeps five items each", () => {
    const view = resultView("attention_summary", {
      data: {
        orders: {
          open: 9,
          items: Array.from({ length: 7 }, (_, i) => ({
            id: `0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5${i}`,
            number: `ORD-${i}`,
            status: "open",
            created_at: "2026-11-01T08:00:00Z",
            total: 1000,
            currency: { code: "USD", decimals: 2 },
          })),
        },
        invoices: null,
        stock: {
          negative: 1,
          out_of_stock: 2,
          items: [{ product: { id: a, name: "AFM Stickers" }, warehouse: { id: b, name: "Main" }, level: "0.00" }],
        },
        till_shifts: { open: 1, items: [{ id: b, register: { id: a, name: "Front" }, opened_at: "2026-11-08T06:00:00Z" }] },
        lots: { quarantined: 1, holds: 0 },
        webhooks: { failed: 1, items: [{ id: a, endpoint_id: b, event_type: "order.paid", attempts: 5, last_error: "timeout", updated_at: "2026-11-08T06:00:00Z" }] },
      },
    })
    if (view?.kind !== "attention") throw new Error("expected attention")
    expect(view.sections.map((s) => s.key)).toEqual(["orders", "stock", "till_shifts", "lots", "webhooks"])
    const orders = view.sections[0]!
    expect(orders.counts).toEqual({ open: 9 })
    expect(orders.items).toHaveLength(5)
    expect(orders.items[0]).toMatchObject({
      label: "ORD-0",
      money: { minor: 1000, decimals: 2, currency: "USD" },
      href: "/orders/0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a50",
    })
    expect(view.sections[1]!.counts).toEqual({ negative: 1, out_of_stock: 2 })
    expect(view.sections[1]!.items[0]).toMatchObject({ label: "AFM Stickers", detail: "Main", level: "0.00", href: `/products/${a}` })
    expect(view.sections[2]!.items[0]).toMatchObject({ label: "Front", at: "2026-11-08T06:00:00Z", href: null })
    expect(view.sections[3]).toEqual({ key: "lots", counts: { quarantined: 1, holds: 0 }, items: [] })
    expect(view.sections[4]!.items[0]).toMatchObject({ label: "order.paid", detail: "timeout", attempts: 5 })
  })

  it("an unexpected shape shows nothing rather than guessing", () => {
    expect(resultView("orders_list", { data: "nope" })).toBeUndefined()
    expect(resultView("orders_sales_summary", { data: { currencies: "x" } })).toBeUndefined()
    expect(resultView("attention_summary", null)).toBeUndefined()
  })
})
