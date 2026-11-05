import type { InvoiceMoney } from "@genesy/api-client/model"
import { formatMoney } from "@genesy/ui/lib/money"

/**
 * Minor units from the API's `money` (computed in SQL) in that currency. Display only: the
 * office never adds, rounds or parses money. A dash when the currency is gone.
 */
export function invoiceMoney(
  minor: number,
  money: Pick<InvoiceMoney, "currency" | "decimals">,
  locale: string
) {
  return money.currency
    ? formatMoney(minor, money.decimals, money.currency, locale)
    : "—"
}

/** The invoice's number; null for a draft and for a cancelled draft (the API prints "N/A"). */
export const invoiceNumber = (nr: string | null | undefined) =>
  nr && nr !== "N/A" ? nr : null
