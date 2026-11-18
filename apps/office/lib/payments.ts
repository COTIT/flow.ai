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
  captured_transaction_id?: string | null
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

/** Unknown provider words stay under review, never read as success. */
export function providerStatusKey(value: unknown) {
  const status = typeof value === "string" ? value.toLowerCase() : "review"
  return [
    "new",
    "created",
    "pending",
    "authorized",
    "processed",
    "failed",
    "refunded",
    "partially_refunded",
    "chargebacked",
    "cancelled",
    "declined",
    "succeeded",
    "open",
    "closed",
  ].includes(status)
    ? status
    : "review"
}

export function paymentTone(status: string): "teal" | "gold" | "red" | "slate" {
  if (
    ["paid", "processed", "succeeded", "refunded", "settled"].includes(status)
  )
    return "teal"
  if (["failed", "declined", "chargebacked"].includes(status)) return "red"
  if (["cancelled", "expired", "closed"].includes(status)) return "slate"
  return "gold"
}

/** Provider decimal amounts are major units; do not multiply or sum attempts. */
export function providerMoney(
  amount: unknown,
  currency: unknown,
  decimals: number,
  locale: string
) {
  if (
    (typeof amount !== "string" && typeof amount !== "number") ||
    !/^-?\d+(?:\.\d+)?$/.test(String(amount)) ||
    typeof currency !== "string" ||
    !/^[A-Z]{3}$/.test(currency)
  )
    return null
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency,
    minimumFractionDigits: decimals,
    maximumFractionDigits: 20,
  }).format(amount as number)
}

/** Offset-less provider timestamps have no known zone: preserve their wall clock. */
export function paymentTimestamp(value: unknown, locale: string) {
  if (
    typeof value !== "string" ||
    !/^\d{4}-\d{2}-\d{2}(?:T\d{2}:\d{2}(?::\d{2}(?:\.\d{1,9})?)?(?:Z|[+-]\d{2}:\d{2})?)?$/.test(
      value
    )
  )
    return null
  const dayOnly = value.length === 10
  const providerTime = !dayOnly && !/(Z|[+-]\d{2}:\d{2})$/.test(value)
  const date = new Date(
    dayOnly ? `${value}T00:00:00Z` : providerTime ? `${value}Z` : value
  )
  if (!Number.isFinite(date.getTime())) return null
  const text = new Intl.DateTimeFormat(locale, {
    year: "numeric",
    month: "short",
    day: "numeric",
    ...(!dayOnly ? ({ hour: "numeric", minute: "2-digit" } as const) : {}),
    ...(providerTime || dayOnly
      ? { timeZone: "UTC" }
      : ({ timeZoneName: "short" } as const)),
  }).format(date)
  return { iso: value, text, providerTime }
}
