// Accounting stack helpers: a fresh workspace of the spec's own (the seeded books stay untouched)
// and, for flows that need an active book, the same SQL activation the API tests use
// (apps/api/tests/api/accounting.rs `activate`) until the activation route exists (M8).
import { execFileSync } from "node:child_process"

import type { APIRequestContext } from "@playwright/test"

import { type Api, apiAs, container, expect, mailTo, unique, urls } from "./stack"

/** The book's 26 roles (docs/api/accounting.md "Setup"). */
const ROLES = [
  "receivables",
  "unbilled_receivables",
  "payables",
  "inventory",
  "grni",
  "revenue",
  "sales_returns",
  "cogs",
  "tax_payable",
  "recoverable_tax",
  "customer_deposits",
  "supplier_advances",
  "bank",
  "cash",
  "provider_clearing",
  "inventory_adjustment",
  "pending_purchase_cost_variance",
  "purchase_return_variance",
  "cash_over_short",
  "realized_fx",
  "unrealized_fx",
  "rounding",
  "opening_equity",
  "retained_earnings",
  "financial_discrepancy",
  "financial_loss",
]

/** A new workspace through signup, its owner verified by the mailed link (data rule 1). */
export async function freshWorkspace(request: APIRequestContext, tag: string) {
  const name = `e2e-${tag}-${unique()}`
  const login = {
    app: `office (${tag})`,
    role: "owner",
    login: `${name}@e2e.test`,
    secret: `E2e!${unique()}Aa1`,
    path: "/login",
  }
  const signup = await request.post(`${urls.api}/v1/create-workspace`, {
    data: {
      first_name: "Book",
      last_name: "Keeper",
      email: login.login,
      password: login.secret,
      workspace_name: name,
    },
  })
  expect(signup.status(), "signup").toBe(201)
  const link = (await mailTo(login.login)).match(/https?:\/\/\S+\/v1\/verify-email\/\S+/)?.[0]
  expect(link, "verification link").toBeTruthy()
  await request.get(link!, { maxRedirects: 0 })
  return { login, api: await apiAs(request, login) }
}

/** A write with the C/A envelope and Idempotency-Key the accounting API requires. */
export async function command<T>(
  request: APIRequestContext,
  api: Api,
  method: "POST" | "PUT" | "PATCH",
  path: string,
  body: Record<string, unknown>
) {
  const key = crypto.randomUUID()
  const res = await request.fetch(`${urls.api}/v1${api.ws(path)}`, {
    method,
    headers: { Authorization: `Bearer ${api.token}`, "Idempotency-Key": key },
    data: { client_ref: key, ...body },
  })
  if (res.status() >= 400) throw new Error(`${method} ${path}: ${res.status()} ${await res.text()}`)
  return ((await res.json()) as { data: T }).data
}

/** The workspace's main currency id. */
export async function mainCurrency(api: Api) {
  const list = await api.get<{ id: string; default_currency: boolean; asset_type: string }[]>(
    api.ws("/currencies?page_size=100")
  )
  const main = list.find((c) => c.default_currency) ?? list[0]
  expect(main, "a currency in the fresh workspace").toBeTruthy()
  return main!
}

/** An account of the book. */
export const addAccount = (
  request: APIRequestContext,
  api: Api,
  code: string,
  name: string,
  type: string
) =>
  command<{ id: string; code: string }>(request, api, "POST", "/accounting/accounts", {
    payload: { code, name, type },
  })

const ymd = (d: Date) => d.toISOString().slice(0, 10)

/**
 * Configures the book (currency, UTC, one account per role, this year and month open) through
 * the API, then flips it to active by SQL in this stack's own database, for this workspace only.
 */
export async function activeBook(request: APIRequestContext, api: Api) {
  const currency = await mainCurrency(api)
  const mappings: Record<string, string> = {}
  for (const [i, role] of ROLES.entries())
    mappings[role] = (await addAccount(request, api, `R${String(i).padStart(2, "0")}`, `Role ${role}`, "asset")).id
  const setup = await api.get<{ version: number }>(api.ws("/accounting/setup"))
  await command(request, api, "PUT", "/accounting/setup", {
    expected_version: setup.version,
    payload: { functional_currency_id: currency.id, timezone: "UTC", account_mappings: mappings },
  })
  const now = new Date()
  const year = await command<{ id: string }>(request, api, "POST", "/accounting/fiscal-years", {
    payload: {
      start_date: `${now.getUTCFullYear()}-01-01`,
      end_date: `${now.getUTCFullYear() + 1}-01-01`,
    },
  })
  const first = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), 1))
  const next = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth() + 1, 1))
  await command(request, api, "POST", "/accounting/periods", {
    payload: { fiscal_year_id: year.id, start_date: ymd(first), end_date: ymd(next) },
  })
  expect(api.workspace).toMatch(/^[0-9a-f-]{36}$/)
  // shortcut: activation by SQL until the M8 activate route (mirrors tests/api/accounting.rs).
  execFileSync(
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
      `UPDATE accounting_settings SET state = 'active', cutover_at = now(), activated_at = now(),
         activated_by = (SELECT created_by_user_id FROM workspaces WHERE id = '${api.workspace}'::uuid)
       WHERE workspace_id = '${api.workspace}'::uuid`,
    ],
    { encoding: "utf8" }
  )
  return { currency, mappings }
}
