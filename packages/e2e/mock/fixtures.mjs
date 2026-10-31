// Ids, codes and settings shared by the mock storefront and the specs.

export const MOCK_PORT = 8199
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
