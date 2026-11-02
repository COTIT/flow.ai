import { randomUUID } from "node:crypto"

import type { APIRequestContext } from "@playwright/test"

import { APP_KEY, MOCK_ORIGIN, PRODUCT, VARIANT } from "../../mock/fixtures.mjs"
import { expect } from "../../src/test"

/** Places a web order straight on the mock storefront API; returns its id. */
export async function placeOrder(
  request: APIRequestContext,
  email = "buyer@e2e.test"
) {
  const response = await request.post(`${MOCK_ORIGIN}/v1/storefront/orders`, {
    headers: {
      Authorization: `Bearer ${APP_KEY}`,
      "Idempotency-Key": randomUUID(),
    },
    data: {
      lines: [
        {
          product_id: PRODUCT.honey,
          product_variant_id: VARIANT.honey500,
          quantity: "1",
        },
      ],
      customer: { email, name: "Ada Buyer" },
    },
  })
  expect(response.status()).toBe(201)
  return ((await response.json()) as { data: { id: string } }).data.id
}
