import { formatMoney } from '@/lib/format'

/** Shown wherever a value is genuinely absent. Never rendered as 0. */
export const UNKNOWN = '—'

/**
 * A count and its cash, both exactly as the view reported them. Formatting
 * only: neither figure is derived here (business-rules §11).
 */
export function countAndAmount(
  count: number | null | undefined,
  amount: number | null | undefined,
  currency: string,
): string {
  if (count == null) return UNKNOWN
  return `${count} · ${formatMoney(amount ?? null, currency)}`
}

/** A figure the database already rounded, grouped for reading. */
export function formatFigure(value: number | null | undefined): string {
  if (value == null) return UNKNOWN
  return Number(value).toLocaleString('en-GB', { maximumFractionDigits: 2 })
}
