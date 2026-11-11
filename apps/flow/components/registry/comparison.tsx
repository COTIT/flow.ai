"use client"

import { useLocale, useTranslations } from "next-intl"

import { formatShare } from "@genesy/ui/lib/format"
import { formatMoney } from "@genesy/ui/lib/money"
import { useCompare } from "@/lib/use-scene-data"
import { DataState, Restricted } from "./data-state"
import type { RegistryProps } from "./types"

/** This period against the one before; the difference and change come from the API. */
export function Comparison({ component, restricted }: RegistryProps) {
  const t = useTranslations("scenes")
  const locale = useLocale()
  const metric = component.metric ?? "sales.gross"
  const period = component.period ?? "this_week"
  const title = `${t(`metric.${metric}`)} · ${t(`period.${period}`)}`
  const query = useCompare(period, !restricted)
  if (restricted) return <Restricted title={title} />
  const net = metric === "sales.net"
  const rows = query.data?.data.currencies ?? []
  return (
    <DataState query={query} empty={!rows.length} emptyTitle={t("noSales")}>
      <div data-comparison="" className="flex flex-col gap-2 rounded-2xl border bg-card p-4">
        <p className="text-xs text-muted-foreground">{title}</p>
        {rows.map((row) => {
          const bp = net ? row.net_change_bp : row.gross_change_bp
          const sign = bp && bp > 0 ? "+" : bp && bp < 0 ? "−" : ""
          return (
            <div key={row.currency} className="flex flex-col">
              <p className="text-2xl font-semibold tabular-nums">
                {formatMoney(net ? row.current.net_total : row.current.gross_total, row.decimals, row.currency, locale)}
              </p>
              <p className="text-xs text-muted-foreground">
                {bp ? t("vsBefore", { change: `${sign}${formatShare(Math.abs(bp), locale)}` }) : t("noChange")}
              </p>
            </div>
          )
        })}
      </div>
    </DataState>
  )
}
