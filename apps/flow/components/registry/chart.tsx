"use client"

import { useId, useState } from "react"
import { useLocale, useTranslations } from "next-intl"

import type { DailyCurrency } from "@genesy/api-client/model"
import { formatDay } from "@genesy/ui/lib/format"
import { formatMoney } from "@genesy/ui/lib/money"
import { Series } from "@genesy/voice/components/results/series"
import { useSeries } from "@/lib/use-scene-data"
import { DataState, Restricted } from "./data-state"
import type { RegistryProps } from "./types"

/** A short day name for a week, a day and month for longer periods (date-only, no time zone shift). */
const dayLabel = (date: string, locale: string, week: boolean) =>
  new Date(`${date}T12:00:00`).toLocaleDateString(
    locale,
    week ? { weekday: "short" } : { day: "numeric", month: "short" }
  )

/**
 * Bars or a line fill the tile's height (at a glance), with the days below and the lowest and
 * highest value beside. The drawing is scale only; every number shown is the API's own value.
 */
function Drawing({
  line,
  cur,
  field,
  locale,
}: {
  line: boolean
  cur: DailyCurrency
  field: "gross" | "net"
  locale: string
}) {
  const values = cur.days.map((d) => d[field])
  const money = (v: number) =>
    formatMoney(v, cur.decimals, cur.currency, locale)
  const week = cur.days.length <= 7
  // Longer periods label the first, middle and last day only.
  const shown = week
    ? cur.days
    : cur.days.filter(
        (_, i, all) =>
          i === 0 || i === all.length - 1 || i === Math.floor(all.length / 2)
      )
  return (
    <figure className="m-0 grid min-h-28 flex-1 grid-cols-[auto_minmax(0,1fr)] grid-rows-[minmax(0,1fr)_auto] gap-x-2 gap-y-1">
      <div className="flex flex-col justify-between text-right text-[11px] text-muted-foreground tabular-nums">
        <span data-range="max">
          {values.length ? money(Math.max(...values)) : null}
        </span>
        <span data-range="min">
          {values.length ? money(Math.min(...values)) : null}
        </span>
      </div>
      <div className="min-h-0 min-w-0 border-b border-l border-border/60">
        <Series line={line} values={values} className="h-full" />
      </div>
      <div
        aria-hidden
        className={`col-start-2 flex text-[11px] text-muted-foreground ${line ? "justify-between" : "justify-around"}`}
      >
        {shown.map((d) => (
          <span key={d.date} data-day={d.date}>
            {dayLabel(d.date, locale, week)}
          </span>
        ))}
      </div>
    </figure>
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
      className="flex h-full flex-col gap-3 rounded-2xl border bg-card p-4"
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
        <div id={tableId} className="flex min-h-0 flex-1 flex-col gap-4">
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
              <Drawing
                key={cur.currency}
                line={line}
                cur={cur}
                field={field}
                locale={locale}
              />
            )
          )}
        </div>
      </DataState>
    </section>
  )
}
