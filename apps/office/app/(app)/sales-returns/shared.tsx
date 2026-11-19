"use client"

import { useTranslations } from "next-intl"

export {
  bookAmount,
  Shell,
  today,
  useAction,
  type Outcome,
} from "../accounting/shared"

/** A return state as its label and badge tone. */
export function useStateBadge() {
  const t = useTranslations("salesReturns.states")
  const tones: Record<string, "gold" | "teal" | "slate" | "red"> = {
    draft: "gold",
    posted: "teal",
    reversed: "slate",
    cancelled: "red",
  }
  return (state: string) => ({
    label: t.has(state) ? t(state) : state,
    tone: tones[state] ?? "slate",
  })
}

/** A label for an API word under `salesReturns.<group>`, or the word itself when unknown. */
export function useWord() {
  const t = useTranslations("salesReturns")
  return (group: string, word: string) =>
    t.has(`${group}.${word}`) ? t(`${group}.${word}`) : word
}

/** Every refusal code a return can get, as the sentence staff read (never the code). */
export function useReturnReasons() {
  const t = useTranslations("salesReturns.reasons")
  const a = useTranslations("accounting.reasons")
  return {
    ...Object.fromEntries(
      [
        "dependent_effects_exist",
        "insufficient_stock",
        "legacy_return_cost_required",
      ].map((c) => [c, t(c)])
    ),
    ...Object.fromEntries(
      [
        "stale_document",
        "accounting_not_configured",
        "accounting_period_closed",
        "unsupported_active_erp_operation",
      ].map((c) => [c, a(c)])
    ),
  }
}
