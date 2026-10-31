import type { CheckoutRequest } from "@genesy/api-client/model"

import { isUuid } from "./passport"

export type ParsedCheckout =
  | { ok: true; body: CheckoutRequest; idempotencyKey: string }
  /** `missing`: required fields left empty; empty list = tampered or malformed input. */
  | { ok: false; missing: string[] }

const REQUIRED = [
  "email",
  "name",
  "line1",
  "postal_code",
  "city",
  "country",
] as const
const OPTIONAL = ["phone", "line2"] as const

/**
 * The checkout form as posted to the server action: shape and size only. Every business
 * rule (stock, prices, product state, address rules) is the API's; its 422 comes back as is.
 */
export function parseCheckout(form: FormData): ParsedCheckout {
  const bad: ParsedCheckout = { ok: false, missing: [] }
  const text = (name: string) => String(form.get(name) ?? "").trim()

  const key = text("key")
  if (!isUuid(key)) return bad

  let raw: unknown
  try {
    raw = JSON.parse(text("lines"))
  } catch {
    return bad
  }
  if (!Array.isArray(raw) || raw.length < 1 || raw.length > 50) return bad
  const lines = []
  for (const l of raw as {
    productId?: unknown
    variantId?: unknown
    quantity?: unknown
  }[]) {
    const { productId, variantId, quantity } = l ?? {}
    if (typeof productId !== "string" || !isUuid(productId)) return bad
    if (
      variantId != null &&
      (typeof variantId !== "string" || !isUuid(variantId))
    )
      return bad
    if (
      !Number.isInteger(quantity) ||
      (quantity as number) < 1 ||
      (quantity as number) > 1000
    )
      return bad
    lines.push({
      product_id: productId,
      product_variant_id: (variantId as string | null) ?? null,
      quantity: String(quantity),
    })
  }

  const fields = Object.fromEntries(
    [...REQUIRED, ...OPTIONAL].map((f) => [f, text(f)])
  ) as Record<(typeof REQUIRED)[number] | (typeof OPTIONAL)[number], string>
  if (Object.values(fields).some((v) => v.length > 255)) return bad
  const missing = REQUIRED.filter((f) => !fields[f])
  if (missing.length) return { ok: false, missing }

  const { email, name, phone, line1, line2, postal_code, city, country } =
    fields
  return {
    ok: true,
    idempotencyKey: key,
    body: {
      lines,
      customer: {
        email,
        name,
        phone: phone || null,
        shipping_address: {
          line1,
          ...(line2 ? { line2 } : {}),
          postal_code,
          city,
          country,
        },
      },
    },
  }
}
