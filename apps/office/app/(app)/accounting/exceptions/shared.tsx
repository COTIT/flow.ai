"use client"

import { useLocale, useTranslations } from "next-intl"

import type { ExceptionResource } from "@verzo/api-client/model"
import { formatMoney } from "@verzo/ui/lib/money"

/** An exception's state as its label and badge tone (pending is the one to act on). */
export function useExceptionBadge() {
  const t = useTranslations("accounting.exceptions.states")
  return (state: string) => ({
    label: t.has(state) ? t(state) : state,
    tone: (state === "pending" ? "gold" : "teal") as "gold" | "teal",
  })
}

/** Why an exception exists, as a sentence (never the code); the API's message otherwise. */
export function useWhy() {
  const t = useTranslations("accounting.exceptions.why")
  return (e: ExceptionResource) =>
    t.has(e.reason_code) ? t(e.reason_code) : e.message
}

/** A label for an API word under `accounting.exceptions.<group>`, or the word itself. */
export function useExceptionWord() {
  const t = useTranslations("accounting.exceptions")
  return (group: string, word: string) =>
    t.has(`${group}.${word}`) ? t(`${group}.${word}`) : word
}

/** Minor units from the API in the observation's currency, formatted exactly (no maths). */
export function useMinor() {
  const locale = useLocale()
  return (minor: number, e: ExceptionResource) =>
    formatMoney(minor, e.decimals, e.currency, locale)
}

/** Refusal codes of retry and resolve, as the sentences staff read. */
export function useExceptionReasons() {
  const t = useTranslations("accounting.exceptions.reasons")
  const a = useTranslations("accounting.reasons")
  return {
    exception_state: t("exception_state"),
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
