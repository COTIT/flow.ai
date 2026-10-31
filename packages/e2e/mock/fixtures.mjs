// Ids, codes and settings shared by the mock storefront and the specs.

import process from "node:process"

/** Shifts every e2e port (mock 8199, shop 3200, office 3201), so two checkouts can run at once. */
export const PORT_OFFSET = Number(process.env.E2E_PORT_OFFSET ?? 0)
export const MOCK_PORT = 8199 + PORT_OFFSET
export const MOCK_ORIGIN = `http://127.0.0.1:${MOCK_PORT}`
/** Test-only app key; the mock refuses any other. Not a secret. */
export const APP_KEY = "gsk_e2e_only_not_a_secret"
/** The upstream body the mock sends with errors; it must never reach a page. */
export const SECRET_BODY = "secret upstream detail"

export const PRODUCT = {
  honey: "11111111-1111-4111-8111-111111111111",
  wax: "22222222-2222-4222-8222-222222222222",
  /** The mock answers 503 for this one. */
  failing: "55555555-5555-4555-8555-555555555555",
}

export const LOT = {
  released: "7K3M9QX2WD",
  recalled: "RECA11ED01",
  /** Valid shape, never published: the API's 404. */
  unknown: "ZZZZZZZZZZ",
  /** The mock answers 503 for this one. */
  failing: "BR0KEN0000",
}

/** Variant ids are UUIDs, as the API's are (the checkout form refuses anything else). */
export const VARIANT = {
  honey250: "a1111111-1111-4111-8111-111111111111",
  honey500: "a2222222-2222-4222-8222-222222222222",
  wax: "a3333333-3333-4333-8333-333333333333",
}

/** More than this many of one line and the mock answers the API's stock 422. */
export const STOCK_LIMIT = 10
/** Orders placed with this email get the API's 503 "no payment provider configured". */
export const NO_PROVIDER_EMAIL = "no-provider@e2e.test"
