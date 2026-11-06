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
  /** Sold by size: product-level price_gross is 0, only the variants carry prices. */
  coffee: "33333333-3333-4333-8333-333333333333",
  /** The mock answers 503 for this one. */
  failing: "55555555-5555-4555-8555-555555555555",
  /** Alone on catalogue page 2, with a name far longer than a card is wide. */
  long: "77777777-7777-4777-8777-777777777777",
  /** AFM honey whose linked batch was recalled after the product listed it. */
  recalledHoney: "afa00001-0000-4000-8000-000000000001",
  /** AFM tea whose linked batch is on hold. */
  heldTea: "afa00005-0000-4000-8000-000000000005",
  /** Accounts specs: the API refuses the shop's key as revoked (401) or short of a scope (403). */
  accountsKeyRevoked: "acc00401-0000-4000-8000-000000000401",
  accountsKeyNoScope: "acc00403-0000-4000-8000-000000000403",
}

/** AFM's shop categories, as `GET /storefront/categories` names them. */
export const CATEGORY = {
  honey: "c1000000-0000-4000-8000-000000000001",
  coffee: "c1000000-0000-4000-8000-000000000002",
  tea: "c1000000-0000-4000-8000-000000000003",
  spices: "c1000000-0000-4000-8000-000000000004",
  baskets: "c1000000-0000-4000-8000-000000000005",
  toys: "c1000000-0000-4000-8000-000000000006",
  art: "c1000000-0000-4000-8000-000000000007",
  books: "c1000000-0000-4000-8000-000000000008",
  apparel: "c1000000-0000-4000-8000-000000000009",
  stickers: "c1000000-0000-4000-8000-00000000000a",
  /** Valid shape, no such category: the API's empty list. */
  unknown: "c1000000-0000-4000-8000-0000000000ff",
}

/** The long product's name, as the API sends it. */
export const LONG_NAME =
  "Ilundo Village Cooperative Extra Rare Single-Origin Wild Miombo Forest Blossom Raw Unfiltered Honey Gift Box"
/** Asking for this catalogue page gets the API's 503. */
export const FAILING_PAGE = 66

export const LOT = {
  released: "7K3M9QX2WD",
  recalled: "RECA11ED01",
  quarantine: "HQ1DHQ1D00",
  /** Released, one village of farmers: the honey's passport on its product page. */
  farmed: "FARMERS001",
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
  coffee250: "a4444444-4444-4444-8444-444444444444",
  coffee1k: "a5555555-5555-4555-8555-555555555555",
  long: "a7777777-7777-4777-8777-777777777777",
}

/** More than this many of one line and the mock answers the API's stock 422. */
export const STOCK_LIMIT = 10
/** Orders placed with this email get the API's 503 "no payment provider configured". */
export const NO_PROVIDER_EMAIL = "no-provider@e2e.test"
/** Orders placed with this email turn paid on their second read, like a Stripe webhook landing late. */
export const LATE_PAYMENT_EMAIL = "late-payment@e2e.test"
/** Orders placed with this email come back already cancelled (expired before payment). */
export const EXPIRED_EMAIL = "expired@e2e.test"
/** Orders placed with this email get a 503 from the payment provider (it is down). */
export const PROVIDER_DOWN_EMAIL = "provider-down@e2e.test"
/** Checkout with this email gets the API's 429 (too many web orders waiting). */
export const BUSY_EMAIL = "busy@e2e.test"
/** Reading this order fails with a 503 (API trouble while loading the order page). */
export const FAILING_ORDER = "66666666-6666-4666-8666-666666666666"
/** Checkout with this email gets a 503 (the API is down when the order is placed). */
export const CHECKOUT_DOWN_EMAIL = "checkout-down@e2e.test"
/** Orders placed with this email are cancelled on the provider's page: back unpaid. */
export const CANCEL_PAYMENT_EMAIL = "cancel-payment@e2e.test"
/** Orders placed with this email come back paid, then refunded and cancelled by the office. */
export const REFUNDED_EMAIL = "refunded@e2e.test"
