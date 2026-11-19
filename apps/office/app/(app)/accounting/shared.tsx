"use client"

import { usePathname } from "next/navigation"
import { useTranslations } from "next-intl"

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

/** First and next first of the current month, as the API's dates (a range's default). */
export const thisMonth = () => {
  const d = new Date()
  const pad = (n: number) => String(n).padStart(2, "0")
  const next = new Date(d.getFullYear(), d.getMonth() + 1, 1)
  return {
    from: `${d.getFullYear()}-${pad(d.getMonth() + 1)}-01`,
    to: `${next.getFullYear()}-${pad(next.getMonth() + 1)}-01`,
  }
}
