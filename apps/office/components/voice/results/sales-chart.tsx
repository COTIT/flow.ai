"use client"

import { useId, useState } from "react"
import { useLocale, useTranslations } from "next-intl"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { formatDate } from "@genesy/ui/lib/format"
import { formatMoney } from "@genesy/ui/lib/money"
import type { ResultView } from "@/lib/voice/views"

type Sales = Extract<ResultView, { kind: "sales" }>

/** Sales per period, one bar group per currency (never mixed). Bars are drawing scale only:
 * every number shown is the API's. The table carries the same numbers for screen readers. */
export function SalesChart({ views }: { views: Sales[] }) {
  const t = useTranslations("voice.results")
  const locale = useLocale()
  const [table, setTable] = useState(false)
  const tableId = useId()
  // Account-local calendar day: the API's own date part, so the browser's zone never moves it.
  const period = (v: Sales) =>
    t("period", {
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
    const top = Math.max(...bars.map((b) => b.row?.gross ?? 0), 1)
    return { currency, bars, top }
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
        <div aria-hidden className="flex flex-col gap-4" data-chart="">
          {groups.map(({ currency, bars, top }) => (
            <div key={currency} className="flex flex-col gap-3">
              {bars.map(({ label, row }) => (
                <div key={label} className="flex flex-col gap-1">
                  <div className="flex flex-wrap items-baseline justify-between gap-x-3 text-xs">
                    <span className="text-muted-foreground">{label}</span>
                    <span className="font-semibold tabular-nums">
                      {row ? formatMoney(row.gross, row.decimals, currency, locale) : "—"}
                      <span className="ml-2 font-normal text-muted-foreground">
                        {t("orders", { count: row?.orders ?? 0 })}
                      </span>
                    </span>
                  </div>
                  {row && row.net !== null ? (
                    <span className="text-xs text-muted-foreground tabular-nums">
                      {t("netBeforeRefunds", { amount: formatMoney(row.net, row.decimals, currency, locale) })}
                    </span>
                  ) : null}
                  <svg viewBox="0 0 100 8" preserveAspectRatio="none" className="h-2.5 w-full">
                    <rect width="100" height="8" rx="4" className="fill-(--hair)" />
                    {row && row.gross > 0 ? (
                      <rect width={Math.max((row.gross / top) * 100, 2)} height="8" rx="4" className="fill-(--gold)" />
                    ) : null}
                  </svg>
                </div>
              ))}
            </div>
          ))}
        </div>
      )}
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
                <td className="py-1 pr-2 text-right">{row ? formatMoney(row.refunded, row.decimals, currency, locale) : "—"}</td>
                <td className="py-1 text-right">{row?.orders ?? 0}</td>
              </tr>
            ))
          )}
        </tbody>
      </table>
    </section>
  )
}
