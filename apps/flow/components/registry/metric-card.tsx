"use client"

import { useLocale, useTranslations } from "next-intl"

import type { Attention, FlowMetric } from "@genesy/api-client/model"
import { formatMoney } from "@genesy/ui/lib/money"
import { useAttention, useSales } from "@/lib/use-scene-data"
import { DataState, Restricted } from "./data-state"
import type { RegistryProps } from "./types"

/** Counts the attention summary already computes; `null` = that section is not readable. */
const COUNTS: Partial<Record<FlowMetric, (a: Attention) => number | null>> = {
  "orders.pending": (a) => a.orders?.open ?? null,
  "orders.draft": (a) => a.orders?.draft ?? null,
  "stock.below_minimum": (a) => a.stock?.below_minimum.count ?? null,
  "invoices.overdue": (a) => a.invoices?.overdue ?? null,
  "invoices.unpaid": (a) => a.invoices?.unpaid ?? null,
}

function Card({
  label,
  sub,
  children,
}: {
  label: string
  sub: string
  children: React.ReactNode
}) {
  return (
    <div
      data-metric-card=""
      className="flex flex-col gap-1 rounded-2xl border bg-card p-4"
    >
      <p className="text-xs text-muted-foreground">
        {label} · {sub}
      </p>
      {children}
    </div>
  )
}

function SalesCard({
  metric,
  period,
}: {
  metric: "sales.gross" | "sales.net"
  period: NonNullable<RegistryProps["component"]["period"]>
}) {
  const t = useTranslations("scenes")
  const locale = useLocale()
  const query = useSales(period)
  const rows = query.data?.data.currencies ?? []
  const label = t(`metric.${metric.replace(".", "_")}`)
  return (
    <DataState query={query} empty={!rows.length} emptyTitle={t("noSales")}>
      <Card label={label} sub={t(`period.${period}`)}>
        {rows.map((row) => (
          <p key={row.currency} className="text-2xl font-semibold tabular-nums">
            {formatMoney(
              metric === "sales.gross" ? row.gross_total : row.net_total,
              row.decimals,
              row.currency,
              locale
            )}
            <span className="ml-2 text-xs font-normal whitespace-nowrap text-muted-foreground">
              {t("orders", { count: row.orders })}
            </span>
          </p>
        ))}
      </Card>
    </DataState>
  )
}

function CountCard({ metric }: { metric: FlowMetric }) {
  const t = useTranslations("scenes")
  const query = useAttention()
  const label = t(`metric.${metric.replace(".", "_")}`)
  const value = query.data ? COUNTS[metric]!(query.data.data) : undefined
  if (value === null) return <Restricted title={label} />
  return (
    <DataState query={query} empty={false} emptyTitle="">
      <Card label={label} sub={t("now")}>
        <p className="text-2xl font-semibold tabular-nums">{value}</p>
      </Card>
    </DataState>
  )
}

/** One number the API computed, with its period. */
export function MetricCard({ component, restricted }: RegistryProps) {
  const t = useTranslations("scenes")
  const metric = component.metric
  if (!metric) return null
  if (restricted)
    return <Restricted title={t(`metric.${metric.replace(".", "_")}`)} />
  if (metric === "sales.gross" || metric === "sales.net")
    return <SalesCard metric={metric} period={component.period ?? "today"} />
  return <CountCard metric={metric} />
}
