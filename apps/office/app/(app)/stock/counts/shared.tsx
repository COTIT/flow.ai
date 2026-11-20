"use client"

import { useTranslations } from "next-intl"

import { PillLinks } from "@/components/list"
import { useCan } from "@/lib/can"

export { Shell, today, useAction, useBookToday, type Outcome } from "../../accounting/shared"

/** The stock views as one pill bar: all rows, below zero, counts and valuation (when allowed). */
export function StockPills({
  view,
}: {
  view: "all" | "negative" | "counts" | "valuation"
}) {
  const t = useTranslations()
  const mayCount = useCan("stocktake.list")
  const mayValue = useCan("accounting.read")
  return (
    <PillLinks
      label={t("stock.title")}
      links={[
        { href: "/stock", label: t("stock.all"), active: view === "all" },
        {
          href: "/stock/negative",
          label: t("stock.negative"),
          active: view === "negative",
        },
        ...(mayCount
          ? [
              {
                href: "/stock/counts",
                label: t("stocktakes.title"),
                active: view === "counts",
              },
            ]
          : []),
        ...(mayValue
          ? [
              {
                href: "/stock/valuation",
                label: t("valuation.title"),
                active: view === "valuation",
              },
            ]
          : []),
      ]}
    />
  )
}

/** A stocktake state as its label and badge tone. */
export function useStateBadge() {
  const t = useTranslations("stocktakes.states")
  const tones: Record<string, "gold" | "teal" | "slate" | "red"> = {
    draft: "slate",
    counting: "gold",
    posted: "teal",
    cancelled: "red",
  }
  return (state: string) => ({
    label: t.has(state) ? t(state) : state,
    tone: tones[state] ?? "slate",
  })
}

/** Every refusal code a count can get, as the sentence staff read. */
export function useCountReasons() {
  const t = useTranslations("stocktakes.reasons")
  const a = useTranslations("accounting.reasons")
  return {
    ...Object.fromEntries(
      [
        "receipt_booking_required",
        "warehouse_paused",
        "stocktake_value_required",
        "stocktake_value_not_allowed",
        "backdated_valuation",
      ].map((c) => [c, t(c)])
    ),
    ...Object.fromEntries(
      [
        "stale_document",
        "accounting_not_configured",
        "accounting_period_closed",
      ].map((c) => [c, a(c)])
    ),
  }
}
