import { decimalToMinor, formatMoney } from "@genesy/ui/lib/money"

type Currency = { asset_type: string; decimals: number } | null | undefined

/**
 * An amount as the API sends it (decimal text, or a JSON number for `balance.open`/`total`) in
 * the invoice's currency, exact. A unit price with more places than the currency keeps them.
 * Without a currency the API's text shows as sent.
 */
export function invoiceMoney(
  amount: string | number,
  currency: Currency,
  locale: string
) {
  const text = String(amount)
  if (!currency) return text
  for (const dp of [currency.decimals, text.split(".")[1]?.length ?? 0]) {
    const minor = decimalToMinor(text, dp)
    if (minor !== null)
      return formatMoney(minor, dp, currency.asset_type, locale)
  }
  return text
}

/** The invoice's number; null for a draft and for a cancelled draft (the API prints "N/A"). */
export const invoiceNumber = (nr: string | null | undefined) =>
  nr && nr !== "N/A" ? nr : null
