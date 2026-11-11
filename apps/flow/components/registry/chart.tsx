"use client"

import { useId, useState } from "react"
import { useLocale, useTranslations } from "next-intl"

import type { DailyCurrency } from "@genesy/api-client/model"
import { formatDay } from "@genesy/ui/lib/format"
import { formatMoney } from "@genesy/ui/lib/money"
import { useSeries } from "@/lib/use-scene-data"
import { DataState, Restricted } from "./data-state"
import type { RegistryProps } from "./types"

/** Bars or a line are drawing scale only; every number shown is the API's. */
function Drawing({
  line,
  cur,
  field,
}: {
  line: boolean
  cur: DailyCurrency
  field: "gross" | "net"
}) {
  const top = Math.max(...cur.days.map((d) => d[field]), 1)
  const step = 100 / Math.max(cur.days.length - 1, 1)
  if (line)
    return (
      <svg
        aria-hidden
        viewBox="0 0 100 40"
        preserveAspectRatio="none"
        className="h-28 w-full text-(--gold)"
      >
        <polyline
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          vectorEffect="non-scaling-stroke"
          points={cur.days
            .map((d, i) => `${i * step},${40 - (d[field] / top) * 38}`)
            .join(" ")}
        />
      </svg>
    )
  return (
    <div aria-hidden className="flex h-28 items-end gap-1">
      {cur.days.map((d) => (
        <div
          key={d.date}
          className="flex-1 rounded-t bg-(--gold)/70"
          style={{ height: `${Math.max((d[field] / top) * 100, 2)}%` }}
        />
      ))}
    </div>
  )
}

export function Chart({
  component,
  restricted,
  line,
}: RegistryProps & { line: boolean }) {
  const t = useTranslations("scenes")
  const locale = useLocale()
  const [table, setTable] = useState(false)
  const tableId = useId()
  const metric = component.metric ?? "sales.gross"
  const period = component.period ?? "last_7_days"
  const title = `${t(`metric.${metric.replace(".", "_")}`)} · ${t(`period.${period}`)}`
  const query = useSeries(period, !restricted)
  if (restricted) return <Restricted title={title} />
  const field = metric === "sales.net" ? "net" : "gross"
  const currencies = query.data?.data.currencies ?? []
  return (
    <section
      aria-label={title}
      data-chart={line ? "line" : "bar"}
      className="flex flex-col gap-3 rounded-2xl border bg-card p-4"
    >
      <div className="flex items-center justify-between gap-3">
        <h3 className="text-sm font-semibold">{title}</h3>
        <button
          type="button"
          aria-pressed={table}
          aria-controls={tableId}
          onClick={() => setTable((on) => !on)}
          className="min-h-11 rounded-full px-3 text-xs font-medium underline-offset-4 hover:underline focus-visible:ring-2 focus-visible:ring-(--gold) focus-visible:outline-none"
        >
          {table ? t("showChart") : t("showTable")}
        </button>
      </div>
      <DataState
        query={query}
        empty={!currencies.length}
        emptyTitle={t("noSales")}
      >
        <div id={tableId} className="flex flex-col gap-4">
          {currencies.map((cur) =>
            table ? (
              <table key={cur.currency} className="w-full text-sm">
                <thead>
                  <tr className="text-left text-xs text-muted-foreground">
                    <th className="font-medium">{t("date")}</th>
                    <th className="text-right font-medium">{t("amount")}</th>
                  </tr>
                </thead>
                <tbody>
                  {cur.days.map((d) => (
                    <tr key={d.date}>
                      <td>{formatDay(d.date, locale)}</td>
                      <td className="text-right tabular-nums">
                        {formatMoney(
                          d[field],
                          cur.decimals,
                          cur.currency,
                          locale
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            ) : (
              <Drawing key={cur.currency} line={line} cur={cur} field={field} />
            )
          )}
        </div>
      </DataState>
    </section>
  )
}
