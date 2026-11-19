import type { CheckoutRequest } from "@verzo/api-client/model"

import { isUuid } from "./passport"
import { COUNTRY_CODES } from "./countries"

export type ParsedCheckout =
  | { ok: true; body: CheckoutRequest; idempotencyKey: string }
  /** `missing`: required fields left empty; empty list = tampered or malformed input. */
  | { ok: false; missing: string[]; errors?: Record<string, string[]> }

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
 * Reject malformed form data before creating an order. The API also validates checkout;
 * its business-rule errors (stock, prices, product state) come back as field messages.
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
  const missing = REQUIRED.filter((f) => !fields[f])
  if (missing.length) return { ok: false, missing }

  const errors: Record<string, string[]> = {}
  for (const [field, value] of Object.entries(fields)) {
    const max = field === "phone" ? 50 : 255
    if ([...value].length > max) {
      const path = [...REQUIRED.slice(2), "line2"].includes(field)
        ? `customer.shipping_address.${field}`
        : `customer.${field}`
      errors[path] = [`Use ${max} characters or fewer.`]
    }
  }
  fields.country = fields.country.toUpperCase()
  if (!COUNTRY_CODES.includes(fields.country))
    errors["customer.shipping_address.country"] = ["Select a valid country."]
  if (Object.keys(errors).length) return { ok: false, missing: [], errors }

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
