"use client"

import { useTranslations } from "next-intl"

/** A bank statement as `bank_statements_get` sends it (accounting.md "Reads"); amounts are exact strings. */
export type Statement = {
  id: string
  bank_account_id: string
  reference: string
  start_date: string
  end_date: string
  opening_balance: string
  closing_balance: string
  line_count: number
  reconciled_lines: number
  unreconciled_lines: number
  unreconciled_amount: string
  state: "open" | "reconciled"
  created_at: string
}

/** A statement line with its live match (`bank_statement_lines`). */
export type StatementLine = {
  id: string
  row_number: number
  transaction_id: string | null
  booking_date: string
  description: string | null
  amount: string
  matched_amount: string
  residual: string
  state: "unreconciled" | "reconciled"
  reconciliation_id: string | null
  reconciliation_version: number | null
  allowed_actions: string[]
  allocations: {
    journal_line_id: string
    journal_entry_id: string
    entry_number: number | null
    accounting_date: string
    amount: string
    journal_reversed: boolean
  }[]
}

/** A journal line a match would accept, with its capacity left (`bank_statements_line_candidates`). */
export type Candidate = {
  journal_line_id: string
  journal_entry_id: string
  entry_number: number | null
  accounting_date: string
  reason: string
  source_ref: string | null
  remaining: string
}

/** A statement or line state as label and badge tone. */
export function useBankBadge() {
  const t = useTranslations("accounting.banking.states")
  return (state: string) => ({
    label: t.has(state) ? t(state) : state,
    tone: (state === "reconciled" ? "teal" : "gold") as "teal" | "gold",
  })
}

/** Every refusal code matching and unmatching can get, as the sentence staff read. */
export function useBankReasons() {
  const t = useTranslations("accounting.banking.reasons")
  const a = useTranslations("accounting.reasons")
  return {
    ...Object.fromEntries(
      [
        "already_reconciled",
        "not_this_bank_account",
        "currency_mismatch",
        "journal_line_reversed",
        "amount_mismatch",
        "capacity_exceeded",
        "already_reversed",
      ].map((c) => [c, t(c)])
    ),
    ...Object.fromEntries(
      [
        "stale_document",
        "accounting_not_configured",
        "unsupported_active_erp_operation",
      ].map((c) => [c, a(c)])
    ),
  }
}
