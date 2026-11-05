import { expect, type FakeApi, paged, test } from "./fake-api"

const at = "2026-10-30T09:00:00Z"
const usd = { id: "cur-usd", asset_type: "USD", decimals: 2, name: "US dollar" }
const product = { id: "prod-honey", name: "AFM Honey 500g" }
const warehouse = { id: "wh-1", name: "Dar es Salaam" }
const register = {
  id: "reg-1",
  name: "Shop till 1",
  warehouse_id: "wh-1",
  currency_id: usd.id,
  key_prefix: "gtk_ab12",
  paired_at: null,
  created_at: at,
}

// What every till list reads beside its own rows.
const tillLookups = (api: FakeApi) =>
  api
    .on("GET", "/workspace/:ws/currencies", () => paged([usd]))
    .on("GET", "/workspace/:ws/warehouses", () => paged([warehouse]))
    .on("GET", "/workspace/:ws/till/registers", () => paged([register]))

/**
 * Every office list: the API path it reads, what it says with no rows, and a row factory
 * whose `name(i)` is the text that row shows. `paged: false` lists answer `{ data }` only.
 */
const lists: {
  route: string
  path: string
  empty: string
  row: (i: number) => object
  name: (i: number) => string
  paged?: boolean
  /** The list's own query on first load (the shell reads some paths for its counts too). */
  query?: string
  setup?: (api: FakeApi) => void
}[] = [
  {
    route: "/orders",
    query: "",
    path: "/workspace/:ws/orders",
    empty: "No orders yet.",
    row: (i) => ({
      id: `ord-${i}`,
      number: `SO-${1000 + i}`,
      status: "open",
      source: "office",
      customer: { name: "Kim Lee", email: null },
      currency: "USD",
      decimals: 2,
      total: 12100,
      paid_at: null,
      created_at: at,
    }),
    name: (i) => `SO-${1000 + i}`,
  },
  {
    route: "/invoices",
    query: "",
    path: "/workspace/:ws/invoices",
    empty: "No invoices yet.",
    row: (i) => ({
      id: `inv-${i}`,
      invoice_nr: `2026-${String(i).padStart(4, "0")}`,
      customer_name: "Acme Imports",
      status: "pending",
      is_credit: false,
      created_at: at,
      due_date: at,
      total_amount: "121.00",
      balance: { open: 21, paid: "100.00", total: 121 },
      money: { currency: "EUR", decimals: 2, subtotal: 10000, vat: 2100, total: 12100, paid: 10000, open: 2100 },
      relationships: { currency: { asset_type: "EUR", decimals: 2 } },
    }),
    name: (i) => `2026-${String(i).padStart(4, "0")}`,
  },
  {
    route: "/lots",
    query: "",
    path: "/workspace/:ws/lots",
    empty: "No batches yet.",
    row: (i) => ({
      id: `lot-${i}`,
      product_id: product.id,
      code: `ILU-${i}`,
      public_code: "7K3M9Q2XRT",
      status: "released",
      produced_on: "2026-09-01",
      best_before: null,
      expires_on: null,
      origin: null,
      created_at: at,
      updated_at: at,
    }),
    name: (i) => `ILU-${i}`,
    setup: (api) => api.on("GET", "/workspace/:ws/products", () => paged([product])),
  },
  {
    route: "/stock",
    path: "/workspace/:ws/products/stocks",
    empty: "No stock rows yet.",
    row: (i) => ({
      id: `st-${i}`,
      stock_lvl: `${i}.25`,
      reserved: 0,
      minimal_stock_level: 0,
      maximum_stock_level: 0,
      meta: null,
      relationships: { product: { ...product, id: `p-${i}`, name: `Jar ${i}` }, warehouse },
    }),
    name: (i) => `Jar ${i}`,
  },
  {
    route: "/stock/negative",
    path: "/workspace/:ws/products/stocks/negative",
    empty: "Nothing below zero.",
    row: (i) => ({
      id: `st-${i}`,
      stock_lvl: `-${i}.00`,
      reserved: 0,
      minimal_stock_level: 0,
      maximum_stock_level: 0,
      meta: null,
      relationships: { product: { ...product, id: `p-${i}`, name: `Jar ${i}` }, warehouse },
    }),
    name: (i) => `Jar ${i}`,
  },
  {
    route: "/supply/links",
    path: "/workspace/:ws/supply-links",
    empty: "No supply links yet.",
    row: (i) => ({
      id: `link-${i}`,
      direction: "incoming",
      partner: { id: `ws-${i}`, name: `Farm ${i}` },
      fields: [],
      status: "live",
      granted_at: at,
      accepted_at: at,
      revoked_at: null,
    }),
    name: (i) => `Farm ${i}`,
  },
  {
    route: "/supply/transfers",
    path: "/workspace/:ws/supply-transfers",
    empty: "No supply transfers yet.",
    row: (i) => ({
      id: `tr-${i}`,
      link_id: "link-1",
      direction: "incoming",
      partner: { id: `ws-${i}`, name: `Partner ${i}` },
      from_lot: { code: `RUA-${i}`, public_code: "7K3M9Q2XRT" },
      quantity: "250.00",
      status: "received",
      dispatched_at: at,
      to_lot: null,
      received_quantity: "250.00",
      received_at: at,
    }),
    name: (i) => `Partner ${i}`,
    setup: (api) =>
      api
        .on("GET", "/workspace/:ws/products", () => paged([product]))
        .on("GET", "/workspace/:ws/warehouses", () => paged([warehouse])),
  },
  {
    route: "/till/registers",
    path: "/workspace/:ws/till/registers",
    empty: "No registers yet.",
    row: (i) => ({ ...register, id: `reg-${i}`, name: `Till ${i}` }),
    name: (i) => `Till ${i}`,
    setup: tillLookups,
  },
  {
    route: "/till/users",
    query: "",
    path: "/workspace/:ws/till/users",
    empty: "No till users yet.",
    row: (i) => ({
      id: `tu-${i}`,
      name: `Cashier ${i}`,
      user_id: null,
      active: true,
      locked: false,
      created_at: at,
    }),
    name: (i) => `Cashier ${i}`,
    paged: false,
  },
  {
    route: "/till/vaults",
    query: "",
    path: "/workspace/:ws/till/vaults",
    empty: "No vaults yet.",
    row: (i) => ({ id: `v-${i}`, name: `Safe ${i}`, currency_id: usd.id, balance: 0, created_at: at }),
    name: (i) => `Safe ${i}`,
    paged: false,
    setup: tillLookups,
  },
  {
    route: "/till/shifts",
    query: "",
    path: "/workspace/:ws/till/shifts",
    empty: "No shifts yet.",
    row: (i) => ({
      id: `sh-${i}`,
      register_id: "reg-1",
      opened_by: "tu-1",
      opened_at: at,
      opening: 5000,
      expected: 5000,
      closed_by: "tu-1",
      closed_at: at,
      counted: 5000 - i,
      difference: -i,
    }),
    // The difference badge shows on a phone too; i cents short.
    name: (i) => `-$0.${String(i).padStart(2, "0")}`,
    setup: tillLookups,
  },
  {
    route: "/till/sales",
    query: "",
    path: "/workspace/:ws/till/sales",
    empty: "No till sales.",
    row: (i) => ({
      order_id: `ord-${i}`,
      number: `POS-${1000 + i}`,
      client_ref: `c-${i}`,
      register_id: "reg-1",
      till_user_id: "tu-1",
      method: "cash",
      total: 2400,
      server_total: 2400,
      price_mismatch: 0,
      mismatched_lines: 0,
      due: 2400,
      rounding: 0,
      tendered: 2400,
      change: 0,
      card_reference: null,
      sold_at: at,
    }),
    name: (i) => `POS-${1000 + i}`,
    setup: tillLookups,
  },
  {
    route: "/settings/app-keys",
    path: "/workspace/:ws/app-keys",
    empty: "No app keys yet.",
    row: (i) => ({
      id: `key-${i}`,
      name: `Key ${i}`,
      kind: "shop",
      scopes: ["catalog:read"],
      prefix: `gk_live_${i}`,
      created_at: at,
      last_used_at: null,
      revoked_at: null,
    }),
    name: (i) => `Key ${i}`,
  },
  {
    route: "/settings/webhooks",
    path: "/workspace/:ws/webhook-endpoints",
    empty: "No webhook endpoints yet.",
    row: (i) => ({
      id: `wh-${i}`,
      url: `https://erp.example.com/hooks/${i}`,
      events: ["stock.changed"],
      description: null,
      active: true,
      created_at: at,
      updated_at: at,
    }),
    name: (i) => `https://erp.example.com/hooks/${i}`,
  },
  {
    route: "/settings/invites",
    path: "/workspace/:ws/invites",
    empty: "No invites yet",
    row: (i) => ({
      invite_id: `inv-${i}`,
      email: `person${i}@example.com`,
      invite_status: "pending",
      invited_by: "romeo@genesy.test",
      created_at: at,
      expired_at: at,
      last_updated_at: at,
      meta: null,
      relationships: { team: null },
    }),
    name: (i) => `person${i}@example.com`,
    setup: (api) => api.on("GET", "/workspace/:ws/teams", () => paged([])),
  },
]

const firstQuery = (l: (typeof lists)[number]) => l.query ?? "page=1"
const firstUrl = (l: (typeof lists)[number]) =>
  `/v1${l.path.replace(":ws", "w1")}${firstQuery(l) ? `?${firstQuery(l)}` : ""}`

const reply = (l: (typeof lists)[number], rows: object[], page = 1) =>
  l.paged === false ? { data: rows } : paged(rows, page)

for (const l of lists) {
  test(`${l.route}: no rows says "${l.empty}"`, async ({ page, api }) => {
    l.setup?.(api)
    api.on("GET", l.path, () => reply(l, []))
    await page.goto(l.route)
    await expect(page.getByText(l.empty, { exact: true }), `${l.route} empty`).toBeVisible()
    await expect(page.getByText(l.name(1), { exact: true })).toHaveCount(0)
  })

  test.describe(() => {
    test.use({
      allow: { responses: [{ url: firstUrl(l), status: 500 }] },
    })

    test(`${l.route}: a list that fails to load says so; Try again brings the rows`, async ({
      page,
      api,
    }) => {
      l.setup?.(api)
      let fail = true
      api.on("GET", l.path, ({ query }) =>
        fail && query.toString() === firstQuery(l)
          ? { status: 500, body: { message: "The server is busy." } }
          : reply(l, [l.row(1)])
      )
      await page.goto(l.route)
      // TanStack retries three times before giving up.
      const alert = page.getByRole("alert").filter({ hasText: "The server is busy." })
      await expect(alert, `${l.route} 500`).toBeVisible({ timeout: 20_000 })
      fail = false
      await alert.getByRole("button", { name: "Try again" }).click()
      await expect(page.getByText(l.name(1), { exact: true }).first()).toBeVisible()
      await expect(alert).toHaveCount(0)
    })
  })

  if (l.paged !== false)
    test(`${l.route}: Next asks the API for page 2 and shows its rows`, async ({ page, api }) => {
      l.setup?.(api)
      const rows = Array.from({ length: 60 }, (_, i) => l.row(i + 1))
      const pages: string[] = []
      api.on("GET", l.path, ({ query }) => {
        pages.push(query.get("page") ?? "1")
        return reply(l, rows, Number(query.get("page") ?? 1))
      })
      await page.goto(l.route)
      await expect(page.getByText("1–50 of 60"), `${l.route} pager`).toBeVisible()
      await page.getByRole("button", { name: "Next" }).click()
      await expect(page.getByText("51–60 of 60")).toBeVisible()
      await expect(page.getByText(l.name(51), { exact: true }).first()).toBeVisible()
      await expect(page.getByText(l.name(1), { exact: true })).toHaveCount(0)
      expect(pages).toContain("2")
      await expect(page.getByRole("button", { name: "Next" })).toBeDisabled()
    })
}
