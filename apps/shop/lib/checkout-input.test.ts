import { describe, expect, it } from "vitest"

import { parseCheckout } from "./checkout-input"

const P = "11111111-1111-4111-8111-111111111111"
const V = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
const KEY = "0b5c7c3e-2f5a-4d7a-9a51-7c1c1f1d2a3b"

const form = (over: Record<string, string> = {}) => {
  const f = new FormData()
  const fields = {
    key: KEY,
    lines: JSON.stringify([{ productId: P, variantId: V, quantity: 2 }]),
    email: "buyer@example.com",
    name: "Ada Buyer",
    phone: "",
    line1: "1 Main St",
    line2: "",
    postal_code: "10001",
    city: "New York",
    country: "US",
    ...over,
  }
  for (const [k, v] of Object.entries(fields)) f.set(k, v)
  return f
}

describe("parseCheckout", () => {
  it("turns the form into the API's checkout body", () => {
    expect(parseCheckout(form())).toEqual({
      ok: true,
      idempotencyKey: KEY,
      body: {
        lines: [{ product_id: P, product_variant_id: V, quantity: "2" }],
        customer: {
          email: "buyer@example.com",
          name: "Ada Buyer",
          phone: null,
          shipping_address: {
            line1: "1 Main St",
            postal_code: "10001",
            city: "New York",
            country: "US",
          },
        },
      },
    })
  })

  it("names each missing required field", () => {
    const r = parseCheckout(form({ email: "", name: " ", city: "" }))
    expect(r).toEqual({ ok: false, missing: ["email", "name", "city"] })
  })

  it.each([
    ["a bad key", { key: "x" }],
    ["lines that aren't JSON", { lines: "{" }],
    ["no lines", { lines: "[]" }],
    [
      "a product id that isn't a UUID",
      { lines: JSON.stringify([{ productId: "../x", quantity: 1 }]) },
    ],
    [
      "a fractional quantity",
      { lines: JSON.stringify([{ productId: P, quantity: 1.5 }]) },
    ],
    [
      "a quantity over 1000",
      { lines: JSON.stringify([{ productId: P, quantity: 1001 }]) },
    ],
    ["an over-long name", { name: "x".repeat(256) }],
  ])("refuses %s", (_, over) => {
    expect(parseCheckout(form(over))).toEqual({ ok: false, missing: [] })
  })
})
