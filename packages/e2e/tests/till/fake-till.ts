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
    // The till reads the register's currency off one storefront row (device key) for now.
    .on("GET", "/storefront/products", () =>
      paged([{ id: "p-honey", currency: "EUR", currency_decimals: 2 }])
    )
    .on("GET", "/workspace/:ws/till/login/users", () => ({
      data: [amina, { id: "tu-2", name: "Baraka" }],
    }))
    .on("POST", "/workspace/:ws/till/login", () => ({ data: session }))
    .on("POST", "/workspace/:ws/till/logout", () => ({ status: 204 }))
  serveCatalog(api, [honey])
  await page.route("**/v1/**", (route) => api.handle(route))
  return api
}

/** Guarded `test` with a faked till API on every page; the device is not seeded. */
export const test = base.extend<{ api: FakeApi }>({
  api: async ({ page }, use) => {
    await use(await fakeTill(page))
  },
})

export { expect, paged }
export type { FakeApi }
