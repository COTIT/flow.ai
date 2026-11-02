import type { Page } from "@playwright/test"

import { test as base, expect } from "../../src/test"
import { FakeApi, paged } from "../office/fake-api"

export const WS = "w1"
export const DEVICE_KEY = "gsk_e2e_device_not_a_secret"
export const TOKEN = "e2e-till-token"

export const register = {
  id: "reg-1",
  name: "Iringa till 1",
  warehouse_id: "wh-shop",
  currency_id: "cur-eur",
  currency: { code: "EUR", decimals: 2 },
  accepted_cash: [5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000],
}
export const amina = { id: "tu-1", name: "Amina" }

export const session = {
  token: TOKEN,
  expires_at: "2099-01-01T00:00:00Z",
  register,
  user: amina,
}

/** A till catalog product (`GET /till/catalog` row); prices excl VAT in the register's EUR. */
export const honey = {
  id: "p-honey",
  name: "Ilundo honey 500 g",
  type: "physical",
  ean13: "8712345678906" as string | null,
  barcode: null as string | null,
  store_only: false,
  vat_rate: "9",
  unit_price: 1147,
  unit_price_gross: 1250 as number | null,
  has_stock: true,
  stock: "7.00",
  sellable: true,
  variants: [] as unknown[],
  updated_at: "2026-11-01T07:00:00Z",
}
export type TillRow = typeof honey

export const NEXT_SINCE = "2026-11-01T09:00:00Z"

/** A till catalog page: paged rows plus the next delta mark. */
export function catalogPage(rows: unknown[], page = 1, perPage = 100) {
  return { ...paged(rows, page, perPage), next_since: NEXT_SINCE }
}

/** Rows carrying exactly `code` (product or variant EAN/barcode), as `?code=` answers. */
export function byCode(rows: TillRow[], code: string) {
  const has = (x: { ean13: string | null; barcode: string | null }) =>
    [x.ean13, x.barcode].some((c) => c?.toLowerCase() === code.toLowerCase())
  return rows.filter(
    (r) =>
      has(r) ||
      (r.variants as { ean13: string | null; barcode: string | null }[]).some(
        has
      )
  )
}

/** Answers `GET /till/catalog`: the whole list, one page, or the `?code=` match. */
export function serveCatalog(api: FakeApi, rows: TillRow[]) {
  api.on("GET", "/workspace/:ws/till/catalog", ({ query }) => {
    const code = query.get("code")
    return catalogPage(code ? byCode(rows, code) : rows)
  })
}

type QuoteBody = {
  lines: { product_id: string; product_variant_id?: string; quantity: string }[]
  payment: { method: string; tendered?: number }
}

/**
 * Answers `POST /till/sales/quote` from the rows' gross prices, without rounding;
 * specs that need the API's cash rounding answer it themselves.
 */
export function serveQuote(api: FakeApi, rows: TillRow[]) {
  const gross = (l: QuoteBody["lines"][number]) => {
    const p = rows.find((r) => r.id === l.product_id)!
    const v = (p.variants as { id: string; unit_price_gross: number }[]).find(
      (x) => x.id === l.product_variant_id
    )
    return (v?.unit_price_gross ?? p.unit_price_gross!) * Number(l.quantity)
  }
  api.on("POST", "/workspace/:ws/till/sales/quote", ({ body }) => {
    const b = body as QuoteBody
    const total = b.lines.reduce((sum, l) => sum + gross(l), 0)
    return quoteReply(b, total, total)
  })
}

/** A quote as the API shapes it, for a basket totalling `total` and paying `due`. */
export function quoteReply(body: QuoteBody, total: number, due: number) {
  const tendered = body.payment.tendered
  return {
    data: {
      lines: [],
      subtotal: total,
      vat: 0,
      total,
      server_total: total,
      price_mismatch: 0,
      mismatched_lines: 0,
      rounding: due - total,
      due,
      change: tendered == null ? null : tendered - due,
    },
  }
}

/** Starts the page paired (and logged in unless `loggedIn: false`), via the till's localStorage entry. */
export async function seedTill(page: Page, { loggedIn = true } = {}) {
  const value = JSON.stringify({
    state: {
      device: { workspaceId: WS, deviceKey: DEVICE_KEY },
      session: loggedIn ? session : null,
    },
    version: 0,
  })
  await page.addInitScript((v) => localStorage.setItem("genesy.till", v), value)
}

/** A FakeApi with the login routes, the catalog and the currency; specs add the rest. */
export async function fakeTill(page: Page) {
  const api = new FakeApi()
    .on("GET", "/workspace/:ws/till/login/users", () => ({
      data: [amina, { id: "tu-2", name: "Baraka" }],
    }))
    .on("POST", "/workspace/:ws/till/login", () => ({ data: session }))
    .on("POST", "/workspace/:ws/till/logout", () => ({ status: 204 }))
    .on("GET", "/workspace/:ws/till/me", () => ({
      data: { expires_at: session.expires_at, register, user: amina },
    }))
  serveCatalog(api, [honey])
  serveQuote(api, [honey])
  await page.route("**/v1/**", (route) => api.handle(route))
  return api
}

/** Guarded `test` with a faked till API on every page; the device is not seeded. */
export const test = base.extend<{ api: FakeApi }>({
  // auto: a spec that never names `api` still gets it, so nothing reaches a real backend.
  api: [
    async ({ page }, use) => {
      await use(await fakeTill(page))
    },
    { auto: true },
  ],
})

export { expect, paged }
export type { FakeApi }
