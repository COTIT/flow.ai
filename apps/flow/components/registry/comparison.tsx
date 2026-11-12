"use client"

import { useLocale, useTranslations } from "next-intl"

import type { FlowPeriod } from "@genesy/api-client/model"
import { formatQty, formatShare } from "@genesy/ui/lib/format"
import { formatMoney } from "@genesy/ui/lib/money"
import { useCompare } from "@/lib/use-scene-data"
import { DataState, Restricted } from "./data-state"
import { RecordTable } from "./record-table"
import type { RegistryProps } from "./types"

/** This period against the one before; the difference and change come from the API. */
export function Comparison({ component, restricted }: RegistryProps) {
  const t = useTranslations("scenes")
  const locale = useLocale()
  const metric = component.metric ?? "sales.gross"
  const period = component.period ?? "this_week"
  const title = `${t(`metric.${metric.replace(".", "_")}`)} · ${t(`period.${period}`)}`
  const query = useCompare(period, !restricted)
  if (restricted) return <Restricted title={title} />
  const net = metric === "sales.net"
  const rows = query.data?.data.currencies ?? []
  return (
    <DataState
      query={query}
      empty={!rows.length}
      emptyTitle={t("noSales")}
      reserve={106}
    >
      <div
        data-comparison=""
        className="flex flex-col gap-2 rounded-2xl border bg-card p-4"
      >
        <p className="text-xs text-muted-foreground">{title}</p>
        {rows.map((row) => {
          const bp = net ? row.net_change_bp : row.gross_change_bp
          const sign = bp && bp > 0 ? "+" : bp && bp < 0 ? "−" : ""
          return (
            <div key={row.currency} className="flex flex-col">
              <p className="text-2xl font-semibold tabular-nums">
                {formatMoney(
                  net ? row.current.net_total : row.current.gross_total,
                  row.decimals,
                  row.currency,
                  locale
                )}
              </p>
              <p className="text-xs text-muted-foreground">
                {bp
                  ? t("vsBefore", {
                      change: `${sign}${formatShare(Math.abs(bp), locale)}`,
                    })
                  : t("noChange")}
              </p>
            </div>
          )
        })}
      </div>
    </DataState>
  )
}

type Line = {
  key: string
  name: string
  current: string
  base: string
  difference: string
  /** The change in percent, when the API gives one. */
  share?: string
}

/**
 * Both periods side by side (Compare with last week): sales, sales excl. VAT and orders, this
 * period, the one before and the difference. Every number is the API's own value.
 */
export function ComparePanel({ period }: { period: FlowPeriod }) {
  const t = useTranslations("scenes")
  const locale = useLocale()
  const query = useCompare(period)
  const rows = query.data?.data.currencies ?? []
  const many = rows.length > 1
  const signed = (n: number, text: string) =>
    n > 0 ? `+${text}` : n < 0 ? `−${text.replace(/^-/, "")}` : text
  const lines: Line[] = rows.flatMap((r) => {
    const money = (v: number) => formatMoney(v, r.decimals, r.currency, locale)
    const suffix = many ? ` (${r.currency})` : ""
    const share = (bp: number | null | undefined) =>
      bp ? signed(bp, formatShare(Math.abs(bp), locale)) : undefined
    return [
      {
        key: `${r.currency}-gross`,
        name: t("metric.sales_gross") + suffix,
        current: money(r.current.gross_total),
        base: money(r.base.gross_total),
        difference: signed(
          r.gross_difference,
          money(Math.abs(r.gross_difference))
        ),
        share: share(r.gross_change_bp),
      },
      {
        key: `${r.currency}-net`,
        name: t("metric.sales_net") + suffix,
        current: money(r.current.net_total),
        base: money(r.base.net_total),
        difference: signed(r.net_difference, money(Math.abs(r.net_difference))),
        share: share(r.net_change_bp),
      },
      {
        key: `${r.currency}-orders`,
        name: t("compare.orders") + suffix,
        current: formatQty(String(r.current.orders), locale),
        base: formatQty(String(r.base.orders), locale),
        difference: signed(
          r.orders_difference,
          formatQty(String(Math.abs(r.orders_difference)), locale)
        ),
      },
    ]
  })
  const title = t(`compare.title.${period}`)
  return (
    <section
      data-compare=""
      aria-label={title}
      className="@container flex flex-col gap-2 rounded-2xl border bg-card p-3 @min-[940px]:p-5"
    >
      <h2 className="px-1 text-sm font-semibold">{title}</h2>
      <DataState query={query} empty={!rows.length} emptyTitle={t("noSales")}>
        <RecordTable
          label={title}
          rows={lines}
          rowKey={(l) => l.key}
          name={(l) => l.name}
          nameLabel={t("compare.what")}
          columns={[
            {
              key: "current",
              label: t(`period.${period}`),
              numeric: true,
              strong: true,
              value: (l) => l.current,
            },
            {
              key: "base",
              label: t(`compare.base.${period}`),
              numeric: true,
              value: (l) => l.base,
            },
            {
              key: "difference",
              label: t("compare.difference"),
              numeric: true,
              value: (l) => (
                <>
                  <span className="whitespace-nowrap">{l.difference}</span>
                  {l.share ? (
                    <span className="block text-xs text-muted-foreground">
                      {l.share}
                    </span>
                  ) : null}
                </>
              ),
            },
          ]}
        />
      </DataState>
    </section>
  )
}
