import { describe, expect, it } from "vitest"
import { resultView } from "./views"

const A = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b"
const B = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5c"

describe("stock pictures", () => {
  it("lists stock rows per warehouse with the API's quantities as written, never computed", () => {
    const view = resultView("stock_list", {
      data: [
        { id: A, stock_lvl: "12.00", reserved: "3.00", available: "9.00", minimal_stock_level: 5, lot: { code: "L-7" },
          relationships: { product: { id: A, name: "Honey 500 g", product_nr: "H-500" }, warehouse: { name: "Main store" } } },
        { id: B, stock_lvl: "-1.00", reserved: "0.00", available: "-1.00", minimal_stock_level: 0, lot: null,
          relationships: { product: { id: B, name: "Wax", product_nr: "W-1" }, warehouse: { name: "Shop" } } },
        { id: "x", stock_lvl: "4.00", reserved: "0.00", available: "4.00", relationships: { product: { id: B, name: "Candle" }, warehouse: { name: "Main store" } } },
      ],
      meta: { total: 31 },
    })
    expect(view).toEqual({
      kind: "stock",
      scope: "list",
      total: 31,
      href: "/stock",
      groups: [
        { title: "Main store", rows: [
          { label: "Honey 500 g", detail: "H-500 · L-7", available: "9.00", level: "12.00", reserved: "3.00", minimum: 5, href: `/products/${A}` },
          { label: "Candle", detail: null, available: "4.00", level: "4.00", reserved: "0.00", minimum: null, href: `/products/${B}` },
        ] },
        { title: "Shop", rows: [
          { label: "Wax", detail: "W-1", available: "-1.00", level: "-1.00", reserved: "0.00", minimum: 0, href: `/products/${B}` },
        ] },
      ],
    })
  })

  it("shows one product's stock per warehouse and bin path, with no available it would have to compute", () => {
    const view = resultView("stock_with_bins", {
      data: [
        { id: A, stock_lvl: "5.00", reserved: "1.00", warehouse: { name: "Main store" }, lot: null,
          shelf: { shelf_name: "Top", shelf_code: "S1", row: { row_name: "Aisle 2", row_code: "R2" } }, bin: { bin_name: "Box 4", bin_code: "B4" } },
        { id: B, stock_lvl: "2.00", reserved: "0.00", warehouse: { name: "Main store" }, lot: { code: "L-9" }, shelf: null, bin: null },
      ],
    })
    expect(view).toEqual({
      kind: "stock",
      scope: "bins",
      total: null,
      href: null,
      groups: [{ title: "Main store", rows: [
        { label: "Aisle 2 › Top › Box 4", detail: null, available: null, level: "5.00", reserved: "1.00", minimum: null, href: null },
        { label: null, detail: "L-9", available: null, level: "2.00", reserved: "0.00", minimum: null, href: null },
      ] }],
    })
  })

  it("an unexpected shape shows nothing", () => {
    expect(resultView("stock_list", { data: "x" })).toBeUndefined()
    expect(resultView("stock_with_bins", { data: {} })).toBeUndefined()
  })
})

describe("quick search", () => {
  it("groups hits as the API ranked them; customers have no page, a hidden group stays hidden", () => {
    expect(
      resultView("search_quick", {
        data: {
          products: [{ id: A, name: "Honey", product_nr: "H-1", ean13: "123" }],
          orders: [{ id: A, number: "ORD-7", reference: null, status: "open", total: 125050, decimals: 2, currency: "USD" }],
          invoices: [{ id: B, number: "2026-001", status: "paid", customer_name: "Neema" }],
          lots: [],
          relations: [{ id: A, name: "Neema Shop" }],
        },
      })
    ).toEqual({
      kind: "search",
      groups: [
        { key: "products", rows: [{ label: "Honey", detail: "H-1", status: null, money: null, href: `/products/${A}` }] },
        { key: "orders", rows: [{ label: "ORD-7", detail: null, status: "open", money: { minor: 125050, decimals: 2, currency: "USD" }, href: `/orders/${A}` }] },
        { key: "invoices", rows: [{ label: "2026-001", detail: "Neema", status: "paid", money: null, href: `/invoices/${B}` }] },
        { key: "lots", rows: [] },
        { key: "relations", rows: [{ label: "Neema Shop", detail: null, status: null, money: null, href: null }] },
      ],
    })
    // null = the caller may not list that kind: no group at all.
    expect(resultView("search_quick", { data: { products: null, orders: [], invoices: null, lots: null, relations: null } }))
      .toEqual({ kind: "search", groups: [{ key: "orders", rows: [] }] })
  })
})

describe("record details", () => {
  it("an order: header facts, lines excl VAT and the API's totals; nothing added up", () => {
    const view = resultView("orders_get", {
      data: {
        id: A, number: "ORD-7", status: "open", customer_name: "Neema Shop", created_at: "2026-11-10T09:00:00Z",
        currency: "USD", decimals: 2, subtotal: 2000, vat: 360, total: 2360, paid_at: null, picked_at: "2026-11-11T10:00:00Z",
        lines: [{ title: "Honey 500 g", quantity: "2.00", unit_price: 1000, line_excl: 2000, line_vat: 360 }],
      },
    })
    expect(view).toEqual({
      kind: "document",
      resource: "orders",
      title: "ORD-7",
      status: "open",
      href: `/orders/${A}`,
      facts: [
        { key: "customer", text: "Neema Shop" },
        { key: "created", date: "2026-11-10T09:00:00Z" },
        { key: "picked", date: "2026-11-11T10:00:00Z" },
      ],
      lineMoney: "excl",
      lines: [{ label: "Honey 500 g", quantity: "2.00", money: { minor: 2000, decimals: 2, currency: "USD" } }],
      totals: [
        { key: "subtotal", money: { minor: 2000, decimals: 2, currency: "USD" } },
        { key: "vat", money: { minor: 360, decimals: 2, currency: "USD" } },
        { key: "total", money: { minor: 2360, decimals: 2, currency: "USD" } },
      ],
    })
  })

  it("an invoice: lines incl VAT and total, paid, open from the API's money", () => {
    const view = resultView("invoices_get", {
      data: {
        id: B, invoice_nr: null, status: "draft", due_date: "2026-12-01", customer_ref: { reference: "PO-9" },
        money: { currency: "TZS", decimals: 0, total: 59000, paid: 9000, open: 50000, subtotal: 50000, vat: 9000 },
        items: [{ title: "Wax", quantity: "1.00", total_minor: 59000 }],
      },
    })
    expect(view).toMatchObject({
      kind: "document",
      resource: "invoices",
      title: null,
      status: "draft",
      href: `/invoices/${B}`,
      facts: [{ key: "reference", text: "PO-9" }, { key: "due", date: "2026-12-01" }],
      lineMoney: "incl",
      lines: [{ label: "Wax", quantity: "1.00", money: { minor: 59000, decimals: 0, currency: "TZS" } }],
      totals: [
        { key: "total", money: { minor: 59000, decimals: 0, currency: "TZS" } },
        { key: "paid", money: { minor: 9000, decimals: 0, currency: "TZS" } },
        { key: "open", money: { minor: 50000, decimals: 0, currency: "TZS" } },
      ],
    })
  })

  it("a product: facts, its price as the API's text trimmed to two places, stock per warehouse", () => {
    const view = resultView("products_get", {
      data: {
        id: A, name: "Honey 500 g", product_nr: "H-500", status: "enabled", ean13: "8712345678901", price_incl: "12.5000",
        total_available_stock: 9, total_reserved_stock: 3,
        stocks: [
          { id: A, stock_lvl: "12.00", reserved: "3.00", warehouse: { name: "Main store" }, bin: { bin_name: "Box 4" } },
          { id: "fallback-x", stock_lvl: "0.00", reserved: "0.00", warehouse: { name: "Shop" }, bin: null },
        ],
      },
    })
    expect(view).toEqual({
      kind: "document",
      resource: "products",
      title: "Honey 500 g",
      status: "enabled",
      href: `/products/${A}`,
      facts: [
        { key: "productNr", text: "H-500" },
        { key: "ean", text: "8712345678901" },
        { key: "priceIncl", text: "12.50" },
        { key: "available", text: "9" },
        { key: "reserved", text: "3" },
      ],
      lineMoney: null,
      lines: [
        { label: "Main store › Box 4", quantity: "12.00", money: null },
        { label: "Shop", quantity: "0.00", money: null },
      ],
      totals: [],
    })
  })

  it("never rounds a price: more than two meaningful decimals stay as written", () => {
    const view = resultView("products_get", { data: { id: A, name: "Saffron", price_incl: "1.2345", stocks: [] } })
    expect(view).toMatchObject({ facts: [{ key: "priceIncl", text: "1.2345" }] })
  })

  it("an unexpected shape shows nothing", () => {
    expect(resultView("orders_get", { data: { number: "x" } })).toBeUndefined()
    expect(resultView("invoices_get", { data: { id: B, money: null } })).toBeUndefined()
    expect(resultView("products_get", { data: { id: "nope", name: "x" } })).toBeUndefined()
  })
})

describe("product price with its currency (when the API sends it)", () => {
  it("keeps the API's decimal text with currency and display decimals; nothing recomputed", () => {
    const view = resultView("products_get", {
      data: { id: A, name: "Honey", price_incl: "11.6600", currency: "EUR", decimals: 2, stocks: [] },
    })
    expect(view).toMatchObject({ facts: [{ key: "priceIncl", price: "11.6600", currency: "EUR", decimals: 2 }] })
  })

  it("without a currency the price stays plain text, never a guessed symbol", () => {
    const view = resultView("products_get", { data: { id: A, name: "Honey", price_incl: "11.6600", currency: null, decimals: null, stocks: [] } })
    expect(view).toMatchObject({ facts: [{ key: "priceIncl", text: "11.66" }] })
  })
})

describe("till", () => {
  const sale = (extra: object) => ({
    order_id: A, number: "ORD-0099", sold_at: "2026-11-12T11:05:00Z", method: "cash", total: 125050, price_mismatch: 0, register_id: B, ...extra,
  })

  it("sales: each as booked, with its total in the API's currency when the reply carries one", () => {
    expect(
      resultView("till_sales_list", {
        data: [sale({ currency: "TZS", decimals: 0 }), sale({ number: "ORD-0100", method: "card", price_mismatch: -500, currency: "TZS", decimals: 0 })],
        meta: { total: 42 },
      })
    ).toEqual({
      kind: "till_sales",
      total: 42,
      href: "/till/sales",
      rows: [
        { label: "ORD-0099", at: "2026-11-12T11:05:00Z", method: "cash", mismatch: false, money: { minor: 125050, decimals: 0, currency: "TZS" }, href: `/orders/${A}` },
        { label: "ORD-0100", at: "2026-11-12T11:05:00Z", method: "card", mismatch: true, money: { minor: 125050, decimals: 0, currency: "TZS" }, href: `/orders/${A}` },
      ],
    })
  })

  it("sales without a currency in the reply show no amount, never a guessed one", () => {
    expect(resultView("till_sales_list", { data: [sale({})], meta: { total: 1 } })).toMatchObject({ rows: [{ money: null }] })
  })

  it("shifts: open with the cash expected so far; closed with counted and the API's difference as short, over or even", () => {
    const cash = { currency: "TZS", decimals: 0 }
    const view = resultView("till_shifts_list", {
      data: [
        { id: A, opened_at: "2026-11-12T06:00:00Z", closed_at: null, opening: 50000, expected: 312000, counted: null, difference: null, ...cash },
        { id: B, opened_at: "2026-11-11T06:00:00Z", closed_at: "2026-11-11T18:00:00Z", opening: 50000, expected: 290000, counted: 289500, difference: -500, ...cash },
        { id: B, opened_at: "2026-11-10T06:00:00Z", closed_at: "2026-11-10T18:00:00Z", opening: 50000, expected: 100000, counted: 100000, difference: 0 },
      ],
      meta: { total: 3 },
    })
    expect(view).toEqual({
      kind: "till_shifts",
      total: 3,
      href: "/till/shifts",
      rows: [
        { opened: "2026-11-12T06:00:00Z", closed: null, state: "open", expected: { minor: 312000, decimals: 0, currency: "TZS" }, counted: null, difference: null },
        { opened: "2026-11-11T06:00:00Z", closed: "2026-11-11T18:00:00Z", state: "short", expected: { minor: 290000, decimals: 0, currency: "TZS" }, counted: { minor: 289500, decimals: 0, currency: "TZS" }, difference: { minor: -500, decimals: 0, currency: "TZS" } },
        // No currency in this reply: the state stays, the amounts don't.
        { opened: "2026-11-10T06:00:00Z", closed: "2026-11-10T18:00:00Z", state: "even", expected: null, counted: null, difference: null },
      ],
    })
  })
})
