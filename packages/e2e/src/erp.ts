// Flow ERP stack seeding for the Office returns and banking specs: real API calls where a route
// exists, one SQL line where none does yet (marked as a shortcut), all in the spec's own workspace.
import { execFileSync } from "node:child_process"

import type { APIRequestContext } from "@playwright/test"

import { activeBook, addAccount, command, mainCurrency } from "./accounting"
import { type Api, container, expect, unique, urls } from "./stack"

/** Today in UTC (the book's timezone in these specs), as the API's date. */
export const today = () => new Date().toISOString().slice(0, 10)

/** One SQL statement in this stack's own database; its plain text output. */
export function psql(sql: string) {
  return execFileSync(
    "docker",
    [
      "exec",
      container,
      "psql",
      "-U",
      "postgres",
      "-d",
      "genesy",
      "-v",
      "ON_ERROR_STOP=1",
      "-Atc",
      sql,
    ],
    { encoding: "utf8" }
  ).trim()
}

/** A write with an Idempotency-Key and a body as given (no envelope added). */
export async function keyed<T>(
  request: APIRequestContext,
  api: Api,
  method: "POST" | "PUT",
  path: string,
  data: unknown
) {
  const res = await request.fetch(`${urls.api}/v1${api.ws(path)}`, {
    method,
    headers: {
      Authorization: `Bearer ${api.token}`,
      "Idempotency-Key": crypto.randomUUID(),
    },
    data,
  })
  if (res.status() >= 400)
    throw new Error(`${method} ${path}: ${res.status()} ${await res.text()}`)
  return ((await res.json()) as { data: T }).data
}

/** One verified 0 % sale tax profile for every category (active books tax only through one). */
export async function saleTaxProfile(request: APIRequestContext, api: Api) {
  const draft = await command<{ id: string; version: number }>(
    request,
    api,
    "POST",
    "/tax-profiles",
    {
      payload: {
        country: "TZ",
        effective_from: "2000-01-01",
        transaction_types: ["sale"],
        product_categories: [],
        components: [
          {
            code: "VAT",
            name: "VAT",
            kind: "percentage",
            rate: "0",
            recoverable: false,
          },
        ],
        exemptions: [],
        tax_point: "invoice",
        evidence_ref: "e2e fixture",
      },
    }
  )
  const review = await keyed<{ review_fingerprint: string }>(
    request,
    api,
    "POST",
    `/tax-profiles/${draft.id}/verify/review`,
    {}
  )
  await command(request, api, "POST", `/tax-profiles/${draft.id}/verify`, {
    expected_version: draft.version,
    payload: {
      review_fingerprint: review.review_fingerprint,
      evidence_ref: "e2e fixture",
    },
  })
}

/**
 * An active book with a posted shipment of 2 "Honey jar" (10 on stock, valued 10.00 a jar) and a
 * draft credit of those 2 units. Returns the ids the specs open.
 */
export async function draftCredit(request: APIRequestContext, api: Api) {
  await api.post(api.ws("/warehouses"), {
    name: "Main store",
    default_warehouse: true,
  })
  const store = (
    await api.get<{ id: string; name: string }[]>(
      api.ws("/warehouses?page_size=100")
    )
  ).find((w) => w.name === "Main store")!
  const { product_id: product } = await api.post<{ product_id: string }>(
    api.ws("/products"),
    {
      name: "Honey jar",
      price: "12.50",
      has_stock: true,
    }
  )
  await api.post(api.ws(`/products/${product}/stocks`), {
    warehouse_id: store.id,
    mutation_value: "10",
    reason: "Opening",
  })
  await activeBook(request, api)
  // shortcut: the opening value activation will seed (M8 route), as tests/api/inventory_valuation.rs `seed_pool`.
  psql(`INSERT INTO inventory_cost_pools (workspace_id, product_id, warehouse_id, quantity, value)
        VALUES ('${api.workspace}'::uuid, '${product}'::uuid, '${store.id}'::uuid, 10, 10000)`)
  await saleTaxProfile(request, api)

  const order = await keyed<{ order_id: string }>(
    request,
    api,
    "POST",
    "/orders",
    {
      lines: [{ product_id: product, quantity: "4" }],
    }
  )
  const confirmed = await keyed<{ lines: { id: string }[] }>(
    request,
    api,
    "POST",
    `/orders/${order.order_id}/confirm`,
    {}
  )
  const line = confirmed.lines[0]!.id
  const shipment = await command<{ id: string; version: number }>(
    request,
    api,
    "POST",
    `/orders/${order.order_id}/shipments`,
    { payload: { lines: [{ order_line_id: line, quantity: "2" }] } }
  )
  const review = await keyed<{ review_fingerprint: string }>(
    request,
    api,
    "POST",
    `/orders/${order.order_id}/shipments/${shipment.id}/post/review`,
    { accounting_date: today() }
  )
  await command(
    request,
    api,
    "POST",
    `/orders/${order.order_id}/shipments/${shipment.id}/post`,
    {
      expected_version: shipment.version,
      payload: {
        accounting_date: today(),
        review_fingerprint: review.review_fingerprint,
      },
    }
  )
  const [recognition, recognitionLine] = psql(
    `SELECT r.id || ',' || l.id FROM sales_recognitions r
       JOIN sales_recognition_lines l ON l.recognition_id = r.id
      WHERE r.workspace_id = '${api.workspace}'::uuid AND r.order_id = '${order.order_id}'::uuid`
  ).split(",")
  expect(recognitionLine, "the shipment's recognition").toBeTruthy()
  const credit = await command<{ id: string }>(
    request,
    api,
    "POST",
    "/sales-returns",
    {
      payload: {
        recognition_id: recognition,
        accounting_date: today(),
        reason: "Cracked jars",
        lines: [{ recognition_line_id: recognitionLine, quantity: "2" }],
      },
    }
  )
  return { order: order.order_id, credit: credit.id, product, store: store.id }
}

/**
 * An active book with bank account "Main bank" (GL 1100), two supplier payments of 40.00 and
 * 25.00 out of it, and an imported statement "ST-<tag>" with lines -40.00, -25.00, -7.00.
 */
export async function bankStatement(request: APIRequestContext, api: Api) {
  await activeBook(request, api)
  const currency = await mainCurrency(api)
  const gl = await addAccount(request, api, "1100", "Main bank", "asset")
  const bank = await command<{ id: string }>(
    request,
    api,
    "POST",
    "/accounting/bank-accounts",
    {
      payload: {
        name: "Main bank",
        currency_id: currency.id,
        account_id: gl.id,
      },
    }
  )
  const types = await api.get<{ id: string }[]>(
    api.ws("/custom-types?domain=relation&is_supplier=1")
  )
  const { relation_id: supplier } = await api.post<{ relation_id: string }>(
    api.ws("/relations"),
    {
      type: "business",
      company_name: `Bees BV ${unique()}`,
      relation_type_id: types[0]!.id,
    }
  )
  for (const amount of ["40.00", "25.00"]) {
    const payload = {
      supplier_id: supplier,
      currency_id: currency.id,
      direction: "payment",
      amount,
      date: today(),
      location: {
        kind: "bank_account",
        bank_account_id: bank.id,
        reference: `TX-${amount}`,
        evidence_ref: "Statement",
        confirmation_method: "bank_statement",
      },
      evidence_ref: "Statement",
    }
    const review = await keyed<{ review_fingerprint: string }>(
      request,
      api,
      "POST",
      "/supplier-payments/review",
      payload
    )
    await command(request, api, "POST", "/supplier-payments", {
      payload: { ...payload, review_fingerprint: review.review_fingerprint },
    })
  }
  const reference = `ST-${unique()}`
  const rows = [
    ["-40.00", "Bees BV invoice 7"],
    ["-25.00", "Bees BV invoice 8"],
    ["-7.00", "Bank fee"],
  ].map(([amount, description], i) => ({
    row_number: i + 1,
    external_key: null,
    fields: { booking_date: today(), amount, description },
  }))
  let batch = await command<{ id: string; version: number }>(
    request,
    api,
    "POST",
    "/imports",
    {
      payload: {
        profile: "bank_statement",
        filename: "statement.csv",
        file_hash: "b".repeat(64),
        expected_rows: rows.length,
        mapping: {
          bank_account_id: bank.id,
          reference,
          start_date: today(),
          end_date: today(),
          opening_balance: "100.00",
          closing_balance: "28.00",
        },
      },
    }
  )
  batch = await command(request, api, "PUT", `/imports/${batch.id}/chunks/0`, {
    expected_version: batch.version,
    payload: { rows },
  })
  const valid = await command<{
    id: string
    version: number
    validation_fingerprint: string
  }>(request, api, "POST", `/imports/${batch.id}/validate`, {
    expected_version: batch.version,
    payload: { expected_chunks: 1 },
  })
  await command(request, api, "POST", `/imports/${batch.id}/apply`, {
    expected_version: valid.version,
    payload: { validation_fingerprint: valid.validation_fingerprint },
  })
  let statement: { id: string } | undefined
  await expect
    .poll(
      async () => {
        statement = (
          await api.get<{ id: string; reference: string }[]>(
            api.ws("/accounting/bank-statements")
          )
        ).find((s) => s.reference === reference)
        return Boolean(statement)
      },
      { timeout: 30_000, message: "the import job stores the statement" }
    )
    .toBe(true)
  return { bank: bank.id, statement: statement!.id, reference }
}
