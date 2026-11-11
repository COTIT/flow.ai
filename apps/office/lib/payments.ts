import { formatMoney } from "@genesy/ui/lib/money"

/** Safe payment fields shared by the public checkout and workspace reconciliation. */
export type PaymentView = {
  payment_id: string
  status: string
  amount: number
  amount_minor?: string
  currency: string
  decimals: number
  checkout_url: string | null
  attention: boolean
  attention_reason?: string | null
  kind?: string
  environment?: string
  invoice_id?: string | null
  credit_note_id?: string | null
  order_id?: string | null
  transactions?: Record<string, unknown>[]
  chargebacks?: Record<string, unknown>[]
}

/** Prefer the exact i64 text; older responses carry only the numeric minor amount. */
export function paymentMoney(
  payment: Pick<
    PaymentView,
    "amount" | "amount_minor" | "decimals" | "currency"
  >,
  locale: string
) {
  return formatMoney(
    BigInt(payment.amount_minor ?? String(payment.amount)),
    payment.decimals,
    payment.currency,
    locale
  )
}

export function hostedPaymentUrl(value: string | null | undefined) {
  if (!value) return null
  try {
    const url = new URL(value)
    return url.protocol === "https:" && !url.username && !url.password
      ? url.href
      : null
  } catch {
    return null
  }
}

export const paymentUncertain = (
  payment: Pick<PaymentView, "status" | "attention">
) => payment.attention || paymentStatusKey(payment.status) === "review"

export function paymentStatusKey(status: string) {
  if (status === "creating") return "created"
  if (status === "canceled") return "cancelled"
  return [
    "pending",
    "open",
    "paid",
    "failed",
    "cancelled",
    "expired",
    "refunded",
    "partially_refunded",
    "settled",
    "created",
    "processing",
  ].includes(status)
    ? status
    : "review"
}

/** Format the server's decimal string directly, preserving its precision. */
export function decimalAmount(
  amount: string | number,
  decimals: number,
  locale: string
) {
  return new Intl.NumberFormat(locale, {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  }).format(amount as number)
}
