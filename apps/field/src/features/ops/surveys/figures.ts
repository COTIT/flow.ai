import type { CountMinor } from '@genesy/api-client/model'
import { formatMoney } from '@genesy/ui/lib/money'

/** Shown wherever a value is genuinely absent. Never rendered as 0. */
export const UNKNOWN = '—'

/**
 * A count and its cash, both exactly as the API reported them. Formatting
 * only: neither figure is derived here (business-rules §11).
 */
export function countAndAmount(
  figure: CountMinor | null | undefined,
  currency: string,
  decimals: number,
  language: string,
): string {
  if (figure == null) return UNKNOWN
  return `${figure.count} · ${formatMoney(figure.minor, decimals, currency, language)}`
}

/** A figure the API already rounded (decimal text), grouped for reading. */
export function formatFigure(value: number | string | null | undefined): string {
  if (value == null) return UNKNOWN
  return Number(value).toLocaleString('en-GB', { maximumFractionDigits: 2 })
}

/**
 * A typed amount ("5,000", "19.99") as integer minor units, by string maths:
 * the digits are joined, never multiplied as a float. Null when the text is
 * not an amount in a currency of `decimals` places; the API's "reward_minor"
 * refusal is then what the admin reads.
 */
export function amountToMinor(text: string, decimals: number): number | null {
  const match = /^(\d+)(?:\.(\d*))?$/.exec(text.replace(/[\s,]/g, ''))
  if (!match) return null
  const fraction = match[2] ?? ''
  if (fraction.length > decimals) return null
  const minor = Number(match[1] + fraction.padEnd(decimals, '0'))
  // Past 2^53 a JS number is no longer an exact integer: refuse rather than send a rounded amount.
  return Number.isSafeInteger(minor) ? minor : null
}

/** Integer minor units as form text ("5000.00"), by string maths. */
export function minorToAmount(minor: number, decimals: number): string {
  const digits = String(minor).padStart(decimals + 1, '0')
  return decimals ? `${digits.slice(0, -decimals)}.${digits.slice(-decimals)}` : digits
}
