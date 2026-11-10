import { expect, it } from "vitest"
import {
  createdFacts,
  reviewChanges,
  reviewFacts,
  reviewWarnings,
} from "./review-facts"

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
            {
              field: "cost",
              currency: { code: "USD", decimals: 2 },
              before: "0.005",
              after: "0.0075",
            },
            { field: "name", before: "Stickers", after: "AFM Stickers" },
          ],
        },
      },
      "products_update",
      "en"
    )
  ).toMatchObject({
    changes: [
      {
        field: "price",
        currency: "USD",
        before: "$0.50",
        after: "$500.50",
        change_bp: 10000000,
        warning: "large_change",
      },
      { field: "cost", currency: "USD", before: "0.005", after: "0.0075" },
      { field: "name", before: "Stickers", after: "AFM Stickers" },
    ],
  })
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
    expect.objectContaining({
      field: "price",
      before: "$0.50",
      after: "$500.50",
      change_bp: 10000000,
    }),
    expect.objectContaining({
      field: "wholesale_price",
      before: null,
      after: "$1.00",
    }),
    expect.objectContaining({
      field: "gross_price",
      before: "0.505",
      after: "0.1",
      change_bp: -8019,
    }),
  ])
})

it("turns a large money change into a plain sentence per language, never a code", () => {
  expect(reviewWarnings(priceUpdate, "en")).toEqual([
    "Price goes from $0.50 to $500.50, more than 10 times the current amount. Check the amount and its unit.",
  ])
  const down = structuredClone(priceUpdate)
  Object.assign(down.facts.changes[0]!, {
    before_minor: 50050,
    after_minor: 50,
    change_bp: -9990,
  })
  expect(reviewWarnings(down, "en")[0]).toBe(
    "Price goes from $500.50 to $0.50, less than a tenth of the current amount. Check the amount and its unit."
  )
  expect(reviewWarnings(priceUpdate, "nl")[0]).toMatch(
    /^Prijs gaat van .+500,50/
  )
  expect(reviewWarnings(priceUpdate, "sw")[0]).toMatch(/^Bei /)
  expect(JSON.stringify(reviewWarnings(priceUpdate, "en"))).not.toContain(
    "large_change"
  )
  expect(reviewWarnings({ facts: {} }, "en")).toEqual([])
})

it("words a scheduled report request: report name, schedule around the hour in the zone, only you", () => {
  const body = {
    report: "sales_summary",
    frequency: "weekly",
    weekday: 1,
    hour: 7,
  }
  expect(
    reviewChanges(
      body,
      undefined,
      "scheduled_reports_create",
      "en",
      "Africa/Nairobi"
    )
  ).toEqual({
    report: "Sales summary",
    schedule: "Every Monday around 07:00 (Africa/Nairobi)",
    recipient: "Only you, at your login email",
  })
  expect(
    reviewChanges(
      { report: "stock", frequency: "daily", hour: 6 },
      undefined,
      "scheduled_reports_create",
      "sw",
      undefined
    )
  ).toEqual({
    report: "Hisa ndogo",
    schedule: "Kila siku karibu 06:00 (UTC)",
    recipient: "Wewe tu, kwa barua pepe yako ya kuingia",
  })
})

it("words a new currency as new values: code, name, symbol and decimals", () => {
  expect(
    createdFacts(
      "currencies_create",
      {
        asset_type: "tzs",
        name: "Tanzanian shilling",
        symbol_suffix: "TSh",
        decimals: 0,
        default_currency: false,
      },
      "en"
    )
  ).toEqual({
    kind: "currency",
    locked: { label: "Tanzanian shilling" },
    changes: [
      { field: "code", before: null, after: "TZS" },
      { field: "name", before: null, after: "Tanzanian shilling" },
      { field: "symbol", before: null, after: "TSh" },
      { field: "decimals", before: null, after: "0" },
    ],
  })
  expect(
    createdFacts(
      "currencies_create",
      {
        asset_type: "EUR",
        name: "Euro",
        symbol_prefix: "€",
        decimals: 2,
        default_currency: true,
      },
      "nl"
    )?.changes
  ).toContainEqual({ field: "default_currency", before: null, after: "Ja" })
  expect(createdFacts("products_create", { name: "A" }, "en")).toBeUndefined()
})

const correction = {
  version: 1,
  operation_id: "farm_supply_correction_request",
  facts: {
    kind: "supply_correction",
    locked: { label: "ILU-H-1" },
    lines: [{ farmer: "Gift Mushi", kg_before: "60.00", kg_after: "40.00" }],
    net_kg: "-20.00",
    reason: "Scale read high",
    currency: { code: "TZS", decimals: 2 },
    farmers: [
      {
        name: "Gift Mushi",
        change_minor: -1_200_000,
        balance_after_minor: -400_000,
      },
      { name: "Juma Ally", change_minor: 0, balance_after_minor: 50_000 },
    ],
    approval: "office",
  },
}

it("shows a delivery correction's money in currency units and who will owe", () => {
  const shown = reviewFacts(
    correction,
    "farm_supply_correction_request",
    "en"
  ) as { farmers: Record<string, unknown>[] }
  expect(shown.farmers[0]).toMatchObject({ name: "Gift Mushi" })
  expect(String(shown.farmers[0]!.change)).toMatch(/12,000\.00/)
  expect(JSON.stringify(shown)).not.toContain("_minor")
  const [warning, ...rest] = reviewWarnings(correction, "en")
  expect(rest).toEqual([])
  expect(warning).toMatch(/^Gift Mushi will owe .*4,000\.00 back\./)
  expect(warning).not.toMatch(/400000|-/)
  expect(reviewWarnings(correction, "sw")[0]).toContain("atadaiwa")
  expect(reviewWarnings(correction, "nl")[0]).toContain("4.000,00")
})

it("warns that publishing replaces the shop design, in the user's language", () => {
  const facts = (operation_id: string) => ({
    version: 1,
    operation_id,
    facts: {
      kind: "storefront_design",
      changes: [
        { field: "heading", label: "Hi", before: "Hi", after: "Hello" },
      ],
    },
  })
  const publish = facts("storefront_design_publish")
  expect(reviewWarnings(publish, "en")).toEqual([
    "Publishing replaces the home page design customers see when design display is on.",
  ])
  expect(reviewWarnings(publish, "nl")[0]).toMatch(/^Publiceren vervangt/)
  expect(reviewWarnings(publish, "sw")[0]).toMatch(/^Kuchapisha/)
  expect(reviewWarnings(publish, "fr")).toEqual(reviewWarnings(publish, "en"))
  expect(reviewWarnings(facts("storefront_design_edit"), "en")).toEqual([])
  expect(reviewWarnings(facts("storefront_design_restore"), "en")).toEqual([])
})
