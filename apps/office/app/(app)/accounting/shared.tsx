"use client"

import { usePathname } from "next/navigation"
import { useTranslations } from "next-intl"

import { useSession } from "@verzo/api-client/auth"
import { useAccountingSetupGet } from "@verzo/api-client/endpoints/accounting"
import { PillLinks } from "@/components/list"

export {
  bookAmount,
  isStale,
  showMoney,
  showRate,
  today,
} from "../purchases/shared"
export { Shell, useAction, type Outcome } from "../purchases/[id]/dialogs"

/** The book's 26 roles, in the order the setup page lists them (accounting.md "Setup"). */
export const roles = [
  "receivables",
  "unbilled_receivables",
  "payables",
  "inventory",
  "grni",
  "revenue",
  "sales_returns",
  "cogs",
  "tax_payable",
  "recoverable_tax",
  "customer_deposits",
  "supplier_advances",
  "bank",
  "cash",
  "provider_clearing",
  "inventory_adjustment",
  "pending_purchase_cost_variance",
  "purchase_return_variance",
  "cash_over_short",
  "realized_fx",
  "unrealized_fx",
  "rounding",
  "opening_equity",
  "retained_earnings",
  "financial_discrepancy",
  "financial_loss",
] as const

export const accountTypes = [
  "asset",
  "liability",
  "equity",
  "income",
  "expense",
] as const

/** A label for an API word under `accounting.<group>`, or the word itself when unknown. */
export function useWord() {
  const t = useTranslations("accounting")
  return (group: string, word: string) =>
    t.has(`${group}.${word}`) ? t(`${group}.${word}`) : word
}

/** Every refusal code the accounting API sends, as the sentence staff read (never the code). */
export function useReasons() {
  const t = useTranslations("accounting.reasons")
  return Object.fromEntries(
    [
      "stale_document",
      "accounting_not_configured",
      "accounting_period_closed",
      "unsupported_active_erp_operation",
      "already_active",
    ].map((c) => [c, t(c)])
  )
}

const sections = [
  "journals",
  "reports",
  "accounts",
  "periods",
  "cash",
  "banking",
  "exceptions",
  "opening",
  "setup",
] as const

/** The Accounting pages as one pill bar under the page title (one sidebar entry). */
export function AccountingTabs() {
  const t = useTranslations("accounting")
  const path = usePathname()
  return (
    <PillLinks
      label={t("sections.label")}
      links={sections.map((s) => ({
        href: `/accounting/${s}`,
        label: t(`sections.${s}`),
        active: path.startsWith(`/accounting/${s}`),
      }))}
    />
  )
}

/** First of `day`'s month and the first of the next (a range's default, `to` exclusive). */
export const monthOf = (day: string) => {
  const [y = 0, m = 1] = day.split("-").map(Number)
  const pad = (n: number) => String(n).padStart(2, "0")
  return {
    from: `${y}-${pad(m)}-01`,
    to: m === 12 ? `${y + 1}-01-01` : `${y}-${pad(m + 1)}-01`,
  }
}

/** The book's current month: today in the accounting timezone, from the API (`business_date`). */
export function useBookMonth() {
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const setup = useAccountingSetupGet(workspaceId, {
    query: { enabled: Boolean(workspaceId), staleTime: 60_000 },
  })
  const day = setup.data?.data.business_date
  return { month: day ? monthOf(day) : undefined, today: day, setup }
}
