"use server"

import { redirect } from "next/navigation"

import { parseCheckout } from "@/lib/checkout-input"
import { isUuid } from "@/lib/passport"
import { createOrder, startPayment, type PaymentResult } from "@/lib/storefront"
import { getShopBase } from "@/lib/shop-scope"
import { shopPath } from "@/lib/shop-path"

export type CheckoutState = {
  missing?: string[]
  /** The API's field messages, keyed by its paths (`lines.0.quantity`, `customer.email`). */
  errors?: Record<string, string[]>
  problem?: "invalid_input" | "busy" | "unavailable"
}

/** Checkout form -> web order (stock held 30 min) -> the order page, which shows the server's totals. */
export async function placeOrder(
  _: CheckoutState,
  form: FormData
): Promise<CheckoutState> {
  const parsed = parseCheckout(form)
  if (!parsed.ok)
    return parsed.missing.length
      ? { missing: parsed.missing }
      : { problem: "invalid_input" }
  const result = await createOrder(parsed.body, parsed.idempotencyKey)
  if (!result.ok)
    return result.kind === "invalid"
      ? { errors: result.errors }
      : { problem: result.kind }
  redirect(shopPath(await getShopBase(), `/orders/${result.order.id}`))
}

export type PayState = {
  problem?: Extract<PaymentResult, { ok: false }>["kind"]
}

/** Sends the customer to the payment provider, which brings them back to the order page. */
export async function payOrder(_: PayState, form: FormData): Promise<PayState> {
  const id = String(form.get("order") ?? "")
  if (!isUuid(id)) return { problem: "not_payable" }
  // shortcut: localhost default for dev; every deploy must set SITE_URL.
  const site = process.env.SITE_URL ?? "http://localhost:3000"
  const result = await startPayment(
    id,
    `${site}${shopPath(await getShopBase(), `/orders/${id}`)}`
  )
  if (!result.ok) return { problem: result.kind }
  // The URL comes from the API (the provider); still, only ever send people to http(s).
  if (!/^https?:\/\//.test(result.checkoutUrl))
    return { problem: "unavailable" }
  redirect(result.checkoutUrl)
}
