"use client"

import { useLocale, useTranslations } from "next-intl"
import { useStore } from "zustand"

import { useRights } from "@genesy/api-client/rights"
import { formatQty } from "@genesy/ui/lib/format"
import { useVoice } from "@genesy/voice/components/provider"
import { statusOf } from "@genesy/voice/lib/capabilities"
import { flowStore } from "@/lib/flow-store"
import { capability, dockAsk } from "@/lib/pills"
import type { RowAction } from "./record-table"

/**
 * Asks Genesy through the dock, as typing it would: a write comes back as the normal review
 * card, never straight to the API. Null while a review waits, without the right, or with no dock.
 */
export function useAskStep(capabilityId: string) {
  const ask = useStore(dockAsk, (s) => s.ask)
  const { may, read } = useRights()
  const held = Boolean(useVoice().review)
  const allowed = read && statusOf(capability(capabilityId), may) === "approval"
  return ask && allowed && !held ? ask : null
}

/**
 * "Restock" on a product below its minimum: names the product and its shortfall (API values) and
 * decides no amount, so Genesy asks what arrived before it prepares the review. The product is
 * selected for Genesy (id only).
 */
export function useRestock() {
  const t = useTranslations("scenes")
  const locale = useLocale()
  const ask = useAskStep("adjust_stock")
  return (row: { id: string; name: string; shortfall: string | null }) =>
    ask
      ? ({
          key: "restock",
          label: t("rowAction.restock"),
          onSelect: () => {
            flowStore.getState().setSelected({ type: "product", ids: [row.id] })
            ask(
              t("rowAction.restockAsk", {
                name: row.name,
                shortfall: row.shortfall
                  ? formatQty(row.shortfall, locale)
                  : "",
              })
            )
          },
        } satisfies RowAction)
      : undefined
}

/** "Open in Office" on one record (a new tab); undefined when Flow knows no Office origin. */
export function useOpenInOffice() {
  const t = useTranslations("scenes")
  const { resolveHref } = useVoice()
  return (path: string) => {
    const href = resolveHref(path)
    return href
      ? ({
          key: "office",
          label: t("rowAction.openInOffice"),
          href,
        } satisfies RowAction)
      : undefined
  }
}
