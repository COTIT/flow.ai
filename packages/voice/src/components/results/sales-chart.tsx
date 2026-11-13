"use client"

import { useId, useState } from "react"
import { useLocale, useTranslations } from "next-intl"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { formatDate } from "@genesy/ui/lib/format"
import { formatMoney } from "@genesy/ui/lib/money"
import type { ResultView } from "../../lib/views"
import { ResultPlot } from "./plot"

type Sales = Extract<ResultView, { kind: "sales" }>

/** Sales per period, one bar group per currency (never mixed). Bars are drawing scale only:
 * every number shown is the API's. The table carries the same numbers for screen readers.
 * `trend`: a series over time (e.g. one view per day) draws one line per currency instead. */
export function SalesChart({ views, trend = false }: { views: Sales[]; trend?: boolean }) {
  const t = useTranslations("voice.results")
  const locale = useLocale()
  const [table, setTable] = useState(false)
  const tableId = useId()
  // Account-local calendar day: the API's own date part, so the browser's zone never moves it.
  const period = (v: Sales) =>
    v.from.slice(0, 10) === v.to.slice(0, 10)
      ? formatDate(v.from.slice(0, 10), locale)
      : t("period", {
          from: formatDate(v.from.slice(0, 10), locale),
          to: formatDate(v.to.slice(0, 10), locale),
        })
  const currencies = [...new Set(views.flatMap((v) => v.rows.map((r) => r.currency)))]
  const source = views[0]?.source
  const title = t(`source.${source && t.has(`source.${source}`) ? source : "all"}`)
  if (!currencies.length)
    return <EmptyState title={t("noSales")} description={t("noSalesHint")} className="py-6" />
  const groups = currencies.map((currency) => {
    const bars = views.map((v) => ({ label: period(v), row: v.rows.find((r) => r.currency === currency) }))
    return { currency, bars }
  })
  return (
    <section aria-label={title} className="flex flex-col gap-3">
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
      {table ? null : (
        <div className="flex min-w-0 flex-col gap-4" data-chart={trend ? "line" : "hbar"}>
          {groups.map(({ currency, bars }) => {
            const decimals = bars.find((bar) => bar.row)?.row?.decimals ?? 2
            return (
              <div key={currency} className="flex min-w-0 flex-col gap-2">
                <span className="text-xs text-muted-foreground">{currency}</span>
                <ResultPlot
                  kind={trend ? "line" : "hbar"}
                  label={`${title} · ${currency}`}
                  formatValue={(value) => formatMoney(value, decimals, currency, locale)}
                  items={bars.map(({ label, row }) => ({
                    label,
                    value: row?.gross ?? null,
                    shown: row ? formatMoney(row.gross, row.decimals, currency, locale) : "—",
                    detail: row ? t("orders", { count: row.orders }) : undefined,
                  }))}
                />
              </div>
            )
          })}
        </div>
      )}
      <div className={table ? "max-w-full overflow-x-auto" : "sr-only"}>
        <table id={tableId} className={table ? "w-full text-left text-xs tabular-nums" : "sr-only"}>
          <thead className="text-muted-foreground">
            <tr>
              <th scope="col" className="py-1 pr-2 font-medium">{t("periodColumn")}</th>
              <th scope="col" className="py-1 pr-2 text-right font-medium">{t("gross")}</th>
              <th scope="col" className="py-1 pr-2 text-right font-medium">{t("net")}</th>
              <th scope="col" className="py-1 pr-2 text-right font-medium">{t("vat")}</th>
              <th scope="col" className="py-1 pr-2 text-right font-medium">{t("refunded")}</th>
              <th scope="col" className="py-1 text-right font-medium">{t("resources.orders")}</th>
            </tr>
          </thead>
          <tbody>
            {groups.flatMap(({ currency, bars }) =>
              bars.map(({ label, row }) => (
                <tr key={`${currency}${label}`} className="border-t border-(--hair)">
                  <th scope="row" className="py-1 pr-2 font-normal">{label}</th>
                  <td className="py-1 pr-2 text-right">{row ? formatMoney(row.gross, row.decimals, currency, locale) : "—"}</td>
                  <td className="py-1 pr-2 text-right">{row && row.net !== null ? formatMoney(row.net, row.decimals, currency, locale) : "—"}</td>
                  <td className="py-1 pr-2 text-right">{row && row.vat !== null ? formatMoney(row.vat, row.decimals, currency, locale) : "—"}</td>
                  <td className="py-1 pr-2 text-right">{row && row.refunded !== null ? formatMoney(row.refunded, row.decimals, currency, locale) : "—"}</td>
                  <td className="py-1 text-right">{row?.orders ?? 0}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </section>
  )
}
