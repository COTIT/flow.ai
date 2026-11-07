import { expect, it } from "vitest"
import { reviewChanges, reviewFacts } from "./review-facts"

const snapshot = {
  version: 1,
  operation_id: "orders_create",
  facts: {
    kind: "order",
    priced: {
      currency: { code: "EUR", decimals: 2 },
      total_minor: 2500,
      lines: [{ quantity: "2", unit_price_minor: 1250 }],
    },
  },
}

it("shows authoritative amounts in currency units while keeping exact quantities", () => {
  expect(reviewFacts(snapshot, "orders_create", "en")).toMatchObject({
    priced: {
      total: "€25.00",
      lines: [{ quantity: "2", unit_price: "€12.50" }],
    },
  })
  const dutch = reviewFacts(snapshot, "orders_create", "nl")
  expect(JSON.stringify(dutch)).toContain("25,00")
  expect(dutch).not.toHaveProperty("version")
})

it("formats existing-order facts whose currency is a code rather than an object", () => {
  expect(
    reviewFacts(
      {
        version: 1,
        operation_id: "orders_refund",
        facts: {
          kind: "order",
          locked: { currency: "USD", decimals: 2, refundable_minor: -5 },
        },
      },
      "orders_refund",
      "en"
    )
  ).toMatchObject({ locked: { refundable: "-$0.05" } })
})

it.each(["usd", "Usd"])(
  "formats native case-insensitive invoice currency %s without rejecting its review",
  (code) => {
    expect(
      reviewFacts(
        {
          version: 1,
          operation_id: "invoices_send",
          facts: {
            kind: "invoice",
            locked: { currency: { code, decimals: 2 }, total_minor: 1250 },
          },
        },
        "invoices_send",
        "en"
      )
    ).toMatchObject({ locked: { total: "$12.50" } })
  }
)

it("never displays a rounded unsafe amount or a snapshot for another operation", () => {
  expect(() =>
    reviewFacts(
      {
        ...snapshot,
        facts: {
          kind: "order",
          priced: {
            currency: { code: "EUR", decimals: 2 },
            total_minor: 9007199254740992,
          },
        },
      },
      "orders_create",
      "en"
    )
  ).toThrow()
  expect(() => reviewFacts(snapshot, "orders_delete", "en")).toThrow()
})

it("projects hidden facts to a localized permission notice without exposing extra fields", () => {
  const output = reviewFacts(
    { factsHiddenByPermission: true, total_minor: 900, private: "never-show" },
    "orders_create",
    "sw"
  )
  expect(JSON.stringify(output)).not.toMatch(/900|never-show|total_minor/)
  expect(JSON.stringify(output)).toContain("ruhusa")
  expect(reviewFacts(undefined, "products_delete", "en")).toBeUndefined()
})

it("formats requested order prices without changing the exact backend request", () => {
  const body = { lines: [{ quantity: "2", unit_price: 1250 }] }
  expect(reviewChanges(body, snapshot, "orders_create", "en")).toEqual({
    lines: [{ quantity: "2", unit_price: "€12.50" }],
  })
  expect(body.lines[0]?.unit_price).toBe(1250)
  expect(
    JSON.stringify(
      reviewChanges(
        body,
        { factsHiddenByPermission: true },
        "orders_create",
        "en"
      )
    )
  ).toContain("1250 minor units")
})
