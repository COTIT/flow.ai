import { expect, it } from "vitest"
import { reviewChanges, reviewFacts, reviewWarnings } from "./review-facts"

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

it("formats before → after money changes exactly, per change's own currency", () => {
  expect(
    reviewFacts(
      {
        version: 1,
        operation_id: "products_update",
        facts: {
          kind: "product",
          changes: [
            {
              field: "price",
              currency: { code: "USD", decimals: 2 },
              before_minor: 50,
              after_minor: 50050,
              change_bp: 10000000,
              warning: "large_change",
            },
            // Sub-cent: exact decimal strings, never rounded to the currency's decimals.
            { field: "cost", currency: { code: "USD", decimals: 2 }, before: "0.005", after: "0.0075" },
            { field: "name", before: "Stickers", after: "AFM Stickers" },
          ],
        },
      },
      "products_update",
      "en"
    )
  ).toMatchObject({
    changes: [
      { field: "price", currency: "USD", before: "$0.50", after: "$500.50", change_bp: 10000000, warning: "large_change" },
      { field: "cost", currency: "USD", before: "0.005", after: "0.0075" },
      { field: "name", before: "Stickers", after: "AFM Stickers" },
    ],
  })
const priceUpdate = {
  version: 1,
  operation_id: "products_update",
  facts: {
    kind: "product",
    before: { currency: { code: "USD", decimals: 2 }, price: "0.5" },
    priced: { currency: { code: "USD", decimals: 2 }, price: "500.5" },
    changes: [
      {
        field: "price",
        currency: { code: "USD", decimals: 2 },
        before_minor: 50,
        after_minor: 50050,
        change_bp: 10000000,
        warning: "large_change",
      },
      {
        field: "wholesale_price",
        currency: { code: "USD", decimals: 2 },
        before_minor: null,
        after_minor: 100,
      },
      {
        field: "gross_price",
        currency: { code: "USD", decimals: 2 },
        before: "0.505",
        after: "0.1",
        change_bp: -8019,
      },
    ],
  },
}

it("formats exact before and after change amounts, keeping a missing old amount empty", () => {
  const facts = reviewFacts(priceUpdate, "products_update", "en")!
  expect(facts.changes).toEqual([
    expect.objectContaining({ field: "price", before: "$0.50", after: "$500.50", change_bp: 10000000 }),
    expect.objectContaining({ field: "wholesale_price", before: null, after: "$1.00" }),
    expect.objectContaining({ field: "gross_price", before: "0.505", after: "0.1", change_bp: -8019 }),
  ])
})

it("turns a large money change into a plain sentence per language, never a code", () => {
  expect(reviewWarnings(priceUpdate, "en")).toEqual([
    "Price goes from $0.50 to $500.50, more than 10 times the current amount. Check the amount and its unit.",
  ])
  const down = structuredClone(priceUpdate)
  Object.assign(down.facts.changes[0]!, { before_minor: 50050, after_minor: 50, change_bp: -9990 })
  expect(reviewWarnings(down, "en")[0]).toBe(
    "Price goes from $500.50 to $0.50, less than a tenth of the current amount. Check the amount and its unit."
  )
  expect(reviewWarnings(priceUpdate, "nl")[0]).toMatch(/^Prijs gaat van .+500,50/)
  expect(reviewWarnings(priceUpdate, "sw")[0]).toMatch(/^Bei /)
  expect(JSON.stringify(reviewWarnings(priceUpdate, "en"))).not.toContain("large_change")
  expect(reviewWarnings({ facts: {} }, "en")).toEqual([])
})
