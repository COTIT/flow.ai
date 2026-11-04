import { expectNoRawValues } from "../../src/test"

import { expect, paged, test } from "./fake-api"

// Every screen that shows an API code or date, fed every code the API can send: the page shows
// words and formatted dates, never `catalog:read`, `partially_paid` or `2026-11-04`.
// Legit codes on these pages: key prefixes (built in), batch and invoice numbers.
const allow = [/\bILU-\d{4}-\d{2}\b/, /\b\d{4}-\d{4}\b/]
const at = "2026-11-04T18:12:49Z"

const product = {
  id: "prod-honey",
  name: "AFM Honey 500g",
  reference: "AFM-500",
  price: "10.33",
  status: "active",
  short_description: null,
  description: null,
  store_only: false,
  labels: [],
  total_available_stock: 120,
  total_reserved_stock: 0,
  last_updated_at: at,
}

test("app keys: kinds and scopes are words, last used is a date", async ({ page, api }) => {
  const key = (kind: string, scopes: string[], i: number) => ({
    id: `key-${i}`,
    name: `Key ${i}`,
    kind,
    scopes,
    prefix: `gsk_00${i}f`,
    created_at: at,
    last_used_at: at,
    revoked_at: null,
  })
  api.on("GET", "/workspace/:ws/app-keys", () =>
    paged([
      key("shop", ["catalog:read", "checkout:write"], 1),
      key("till", ["catalog:read", "till:login"], 2),
      key("field", [], 3),
      key("partner", ["catalog:read"], 4),
    ])
  )
  await page.goto("/settings/app-keys")
  await expect(page.getByText("Web shop").first()).toBeVisible()
  await expect(page.getByText("Take web orders").first()).toBeAttached()
  await expectNoRawValues(page, { allow })
})

test("webhooks: events are words", async ({ page, api }) => {
  api.on("GET", "/workspace/:ws/webhook-endpoints", () =>
    paged([
      {
        id: "wh-1",
        url: "https://erp.example.com/hooks",
        events: [
          "lot.created",
          "order.created",
          "passport.published",
          "product.changed",
          "sale.attributed",
          "stock.changed",
        ],
        description: null,
        active: true,
        created_at: at,
        updated_at: at,
      },
    ])
  )
  await page.goto("/settings/webhooks")
  await expect(page.getByText("Stock changed").first()).toBeVisible()
  await expectNoRawValues(page, { allow })
})

const statuses = ["draft", "sent", "open", "partially_paid", "paid", "cancelled", "credited"]
const methods = ["cash", "pin", "manual", "provider", "bank"]
const invoice = (status: string, i: number) => ({
  id: `inv-${i}`,
  invoice_nr: `2026-000${i}`,
  customer_name: "Acme Imports",
  status,
  is_credit: false,
  created_at: at,
  due_date: "2026-11-13",
  total_amount: "121.00",
  balance: { open: 21, paid: "100.00", total: 121 },
  items: [],
  relationships: {
    payments: methods.map((m, j) => ({
      id: `pay-${j}`,
      date: "2026-10-31",
      payment_method: m,
      amount: "20.00",
    })),
  },
})

test("invoices: statuses, payment methods and dates are words", async ({ page, api }) => {
  api
    .on("GET", "/workspace/:ws/invoices", () => paged(statuses.map(invoice)))
    .on("GET", "/workspace/:ws/invoices/:id", () => ({
      data: invoice("partially_paid", 4),
    }))
  await page.goto("/invoices")
  await expect(page.getByText("Partly paid").first()).toBeVisible()
  await expectNoRawValues(page, { allow })
  await page.goto("/invoices/inv-4")
  await expect(page.getByText("Bank transfer").first()).toBeVisible()
  await expectNoRawValues(page, { allow })
})

test("product: stock movement types are words", async ({ page, api }) => {
  api
    .on("GET", "/workspace/:ws/products/:id", () => ({ data: product }))
    .on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => ({ data: [] }))
    .on("GET", "/workspace/:ws/products/:id/stock-movements", () =>
      paged(
        ["increase", "decrease", "set_level", "set_requirements"].map((t, i) => ({
          id: `m-${i}`,
          movement_type: t,
          quantity_change: "1.00",
          quantity_before: "1.00",
          quantity_after: "2.00",
          reason: null,
          warehouse: null,
          bin: null,
          created_at: at,
        }))
      )
    )
    .on("GET", "/workspace/:ws/warehouses", () => paged([]))
    .on("GET", "/workspace/:ws/products/:id/images", () => ({ data: [] }))
    .on("GET", "/workspace/:ws/files", () => paged([]))
  await page.goto("/products/prod-honey")
  await expect(page.getByText("Count set").first()).toBeVisible()
  await expectNoRawValues(page, { allow })
})

test("batch: event kinds are words, times are dates", async ({ page, api }) => {
  const kinds = [
    "created",
    "updated",
    "status_changed",
    "passport_published",
    "passport_unpublished",
    "packed",
    "dispatched",
  ]
  api
    .on("GET", "/workspace/:ws/lots/:lot", () => ({
      data: {
        id: "lot-1",
        product_id: product.id,
        code: "ILU-2026-01",
        public_code: "7K3M9Q2XRT",
        status: "released",
        produced_on: "2026-09-01",
        best_before: "2028-09-01",
        expires_on: null,
        origin: "Ilundo, Tanzania",
        created_at: at,
        updated_at: at,
      },
    }))
    .on("GET", "/workspace/:ws/lots/:lot/events", () =>
      paged(
        kinds.map((kind, i) => ({
          id: `ev-${i}`,
          kind,
          from_status: kind === "status_changed" ? "quarantine" : null,
          to_status: "released",
          user_id: null,
          meta: null,
          created_at: at,
        }))
      )
    )
    .on("GET", "/workspace/:ws/products/:id", () => ({ data: product }))
    .on("GET", "/workspace/:ws/lots/:lot/passport", () => ({ data: null }))
  await page.goto("/lots/lot-1")
  await expect(page.getByText("Passport withdrawn").first()).toBeVisible()
  await expectNoRawValues(page, { allow })
})

test("invites: statuses are words, dates are formatted", async ({ page, api }) => {
  api
    .on("GET", "/workspace/:ws/invites", () =>
      paged(
        ["pending", "accepted", "denied", "expired"].map((status, i) => ({
          invite_id: `inv-${i}`,
          email: `amina${i}@example.com`,
          invite_status: status,
          invited_by: "romeo@genesy.test",
          created_at: at,
          expired_at: at,
          last_updated_at: at,
          meta: null,
          relationships: { team: null },
        }))
      )
    )
    .on("GET", "/workspace/:ws/users/teams", () => paged([]))
  await page.goto("/settings/invites")
  await expect(page.getByText("amina0@example.com")).toBeVisible()
  await expectNoRawValues(page, { allow })
})
