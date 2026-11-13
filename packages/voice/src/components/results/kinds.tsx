"use client"

import { useId, useState } from "react"
import { useLocale, useTranslations } from "next-intl"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { formatDate, formatShare } from "@genesy/ui/lib/format"
import { formatMoney } from "@genesy/ui/lib/money"
import { fieldName, formatPrice, formatQuantity } from "../../lib/helpers"
import { drawnAs, shapeOf, type Kind, type ResultView } from "../../lib/views"
import { ResultPlot, type PlotKind } from "./plot"

/** One drawn value: `value` is the API's number (drawing scale only), `shown` its formatted text. */
type Item = { label: string; value: number | null; shown: string }
type Group = { key: string; title: string | null; items: Item[]; change?: string | null; formatValue?: (value: number) => string }

const signed = (bp: number, locale: string) => `${bp > 0 ? "+" : bp < 0 ? "−" : ""}${formatShare(Math.abs(bp), locale)}`

/** Any result as labelled groups (one per currency, section or list), every figure as the API sent it. */
function useGroups(views: ResultView[], kind: Kind): { title: string; groups: Group[] } {
  const t = useTranslations("voice.results")
  const locale = useLocale()
  const day = (iso: string) => formatDate(iso.slice(0, 10), locale)
  const when = (iso: string) => new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(new Date(iso))
  const period = (from: string, to: string) =>
    from.slice(0, 10) === to.slice(0, 10) ? day(from) : t("period", { from: day(from), to: day(to) })
  const money = (minor: number, decimals: number, currency: string) => formatMoney(minor, decimals, currency, locale)
  const count = (n: number) => new Intl.NumberFormat(locale).format(n)
  const view = views[0]!
  switch (view.kind) {
    case "sales_daily":
      return {
        title: `${t("source.all")} · ${period(view.from, view.to)}`,
        groups: view.currencies.map((c) => ({
          key: c.currency,
          title: c.currency,
          formatValue: (value) => money(value, c.decimals, c.currency),
          items: c.days.map((d) => ({ label: day(d.date), value: d.gross, shown: money(d.gross, c.decimals, c.currency) })),
        })),
      }
    case "sales": {
      const sales = views.filter((v): v is Extract<ResultView, { kind: "sales" }> => v.kind === "sales")
      const currencies = [...new Set(sales.flatMap((v) => v.rows.map((r) => r.currency)))]
      return {
        title: t(`source.${view.source && t.has(`source.${view.source}`) ? view.source : "all"}`),
        groups: currencies.map((currency) => ({
          key: currency,
          title: currency,
          formatValue: (value) => money(value, sales.flatMap((v) => v.rows).find((r) => r.currency === currency)!.decimals, currency),
          items: sales.flatMap((v) => {
            const row = v.rows.find((r) => r.currency === currency)
            return row ? [{ label: period(v.from, v.to), value: row.gross, shown: money(row.gross, row.decimals, currency) }] : []
          }),
        })),
      }
    }
    case "comparison":
      return {
        title: t("compare.title"),
        groups: view.currencies.map((c) => ({
          key: c.currency,
          title: c.currency,
          formatValue: (value) => money(value, c.decimals, c.currency),
          change: c.change_bp === null ? null : signed(c.change_bp, locale),
          // kpi shows the current period only; the base is never folded into it.
          items: [
            { label: period(view.from, view.to), value: c.current, shown: money(c.current, c.decimals, c.currency) },
            ...(kind === "kpi" ? [] : [{ label: period(view.base_from, view.base_to), value: c.base, shown: money(c.base, c.decimals, c.currency) }]),
          ],
        })),
      }
    case "breakdown":
      return {
        title: `${t(`breakdown.${view.by}`)} · ${period(view.from, view.to)}`,
        groups: view.currencies.map((c) => ({
          key: c.currency,
          title: c.currency,
          formatValue: (value) => money(value, c.decimals, c.currency),
          items: c.rows.map((r) => ({ label: r.label ?? t("breakdown.walkIn"), value: r.gross, shown: money(r.gross, c.decimals, c.currency) })),
        })),
      }
    case "lot_origin":
      return {
        title: view.title ?? t("originTitle"),
        groups: [{
          key: "materials",
          title: null,
          formatValue: (value) => formatShare(value, locale),
          items: view.materials.map((m) => ({ label: m.name, value: m.share, shown: m.share === null ? "—" : formatShare(m.share, locale) })),
        }],
      }
    case "attention":
      return {
        title: t("attentionTitle"),
        groups: view.sections.map((s) => ({
          key: s.key,
          title: t(`sections.${s.key}`),
          items: Object.entries(s.counts).map(([key, n]) => ({
            label: t.has(`counts.${key}`) ? t(`counts.${key}`) : fieldName(key),
            value: n,
            shown: count(n),
          })),
        })),
      }
    case "records":
      return {
        title: t(`resources.${view.resource}`),
        groups: [{
          key: view.resource,
          title: null,
          // The API's own total (meta.total), never a count of the rows shown.
          items:
            kind === "kpi"
              ? [{ label: t(`resources.${view.resource}`), value: view.total, shown: count(view.total) }]
              : view.rows.map((r) => ({
                  label: r.label,
                  value: null,
                  shown: r.money ? money(r.money.minor, r.money.decimals, r.money.currency) : r.status ? fieldName(r.status) : r.detail ?? "",
                })),
        }],
      }
    case "stock":
      return {
        title: t(view.scope === "list" ? "stock.title" : "stock.perBin"),
        groups: view.groups.map((g, i) => ({
          key: `${g.title}${i}`,
          title: g.title ?? t("stock.noWarehouse"),
          // Number() only sizes the bar; the text shown is the API's own.
          items: g.rows.map((r) => ({ label: r.label ?? t("stock.noBin"), value: Number(r.available ?? r.level), shown: formatQuantity(r.available ?? r.level, locale) })),
        })),
      }
    case "search":
      return {
        title: t("search.title"),
        groups: view.groups.map((g) => ({
          key: g.key,
          title: t(`search.${g.key}`),
          items: g.rows.map((r) => ({
            label: r.label,
            value: null,
            shown: r.money ? money(r.money.minor, r.money.decimals, r.money.currency) : r.status ? fieldName(r.status) : (r.detail ?? ""),
          })),
        })),
      }
    case "document": {
      const cash = (m: { minor: number; decimals: number; currency: string }) => money(m.minor, m.decimals, m.currency)
      const title = `${t(`resources.${view.resource}`)} ${view.title ?? t("document.draft")}`
      if (kind === "kpi") {
        // The figure that matters: an order's total, an invoice's open amount, a product's available stock.
        const last = view.totals.at(-1)
        const available = view.facts.find((f) => f.key === "available")
        const item = last
          ? { label: t(`totals.${last.key}`), value: last.money.minor, shown: cash(last.money) }
          : available && "text" in available
            ? { label: t("facts.available"), value: null, shown: available.text }
            : null
        return { title, groups: [{ key: "kpi", title: null, items: item ? [item] : [] }] }
      }
      return {
        title,
        groups: [
          {
            key: "facts",
            title: null,
            items: view.facts.map((f) => ({ label: t(`facts.${f.key}`), value: null, shown: "text" in f ? f.text : "price" in f ? formatPrice(f.price, f.decimals, f.currency, locale) : day(f.date) })),
          },
          {
            key: "lines",
            title: null,
            items: view.lines.map((l) => ({ label: l.label ?? t("stock.noBin"), value: null, shown: l.money ? cash(l.money) : l.quantity ? formatQuantity(l.quantity, locale) : "" })),
          },
          { key: "totals", title: null, items: view.totals.map((x) => ({ label: t(`totals.${x.key}`), value: x.money.minor, shown: cash(x.money) })) },
        ],
      }
    }
    case "till_sales":
    case "till_shifts": {
      const sales = view.kind === "till_sales"
      const title = t(sales ? "till.sales" : "till.shifts")
      const cash = (m: { minor: number; decimals: number; currency: string } | null) => (m ? money(m.minor, m.decimals, m.currency) : "")
      if (kind === "kpi")
        // The API's own count (meta.total), never a sum of amounts.
        return { title, groups: [{ key: "kpi", title: null, items: [{ label: title, value: view.total, shown: count(view.total) }] }] }
      return {
        title,
        groups: [{
          key: view.kind,
          title: null,
          items:
            view.kind === "till_sales"
              ? view.rows.map((r) => ({ label: `${r.label} · ${when(r.at)}`, value: null, shown: cash(r.money) }))
              : view.rows.map((r) => ({ label: `${when(r.opened)} · ${t(`till.state.${r.state}`)}`, value: null, shown: cash(r.state === "open" ? r.expected : r.counted) })),
        }],
      }
    }
    case "trail":
      return {
        title: t("trailTitle"),
        groups: [{
          key: "events",
          title: null,
          items: view.events.map((e) => ({
            label: t.has(`events.${e.kind}`) ? t(`events.${e.kind}`) : fieldName(e.kind),
            value: null,
            shown: day(e.at),
          })),
        }],
      }
  }
}

// One brand colour stepped down per rank; legends repeat it beside every amount.
const shade = (i: number) => Math.max(1 - i * 0.12, 0.25)

function Legend({ items, swatch }: { items: Item[]; swatch: boolean }) {
  return (
    <ul className="flex min-w-0 flex-1 flex-col gap-1 text-xs">
      {items.map((item, i) => (
        <li key={i} className="flex items-baseline gap-2">
          {swatch ? <span aria-hidden className="size-2.5 shrink-0 rounded-full bg-(--gold)" style={{ opacity: shade(i) }} /> : null}
          <span className="min-w-0 break-words">{item.label}</span>
          <span className="ml-auto font-semibold tabular-nums">{item.shown}</span>
        </li>
      ))}
    </ul>
  )
}

function Drawing({ kind, group, t }: { kind: Kind; group: Group; t: ReturnType<typeof useTranslations> }) {
  const { items } = group
  switch (kind) {
    case "line":
    case "sparkline":
    case "bar":
    case "hbar":
    case "pie":
    case "donut":
      return (
        <div className="flex min-w-0 flex-col gap-3">
          <ResultPlot items={items} kind={kind} label={items.map((i) => `${i.label}: ${i.shown}`).join("; ")} formatValue={group.formatValue} />
          {kind === "pie" || kind === "donut" || kind === "hbar" ? <Legend items={items} swatch={kind !== "hbar"} /> : null}
        </div>
      )
    case "kpi":
      return (
        <div className="flex flex-wrap gap-3">
          {items.map((item, i) => (
            <div key={i} className="flex min-w-32 flex-col rounded-xl border border-(--hair) px-3 py-2">
              <span className="text-xs text-muted-foreground">{item.label}</span>
              <span className="text-xl font-semibold tabular-nums">{item.shown}</span>
            </div>
          ))}
        </div>
      )
    case "delta":
      return (
        <div className="flex flex-col gap-1">
          <span className="text-2xl font-semibold tabular-nums">{items[0]?.shown}</span>
          <span className="text-xs text-muted-foreground">
            {group.change ? t("compare.change", { change: group.change, period: items[1]?.label ?? "" }) : t("compare.noBase", { period: items[1]?.label ?? "" })}
          </span>
          {items[1] ? <span className="text-xs text-muted-foreground tabular-nums">{items[1].label}: {items[1].shown}</span> : null}
        </div>
      )
    case "meter":
      return (
        <ul className="flex flex-col gap-2">
          {items.map((item, i) => (
            <li key={i} className="flex flex-col gap-1">
              <span className="flex items-baseline justify-between gap-3 text-xs">
                <span className="min-w-0 break-words">{item.label}</span>
                <span className="font-semibold tabular-nums">{item.shown}</span>
              </span>
              {/* Basis points of a whole the API defines: 10000 is the full bar. */}
              <svg aria-hidden viewBox="0 0 100 8" preserveAspectRatio="none" className="h-2.5 w-full">
                <rect width="100" height="8" rx="4" className="fill-(--hair)" />
                {item.value ? <rect width={Math.min(Math.max(item.value / 100, 2), 100)} height="8" rx="4" className="fill-(--gold)" /> : null}
              </svg>
            </li>
          ))}
        </ul>
      )
    default:
      return <Legend items={items} swatch={false} />
  }
}

/** A result drawn as the asked kind (already checked against what its shape supports). */
export function KindChart({ views }: { views: ResultView[] }) {
  const t = useTranslations("voice.results")
  const first = views[0]!
  const kind = drawnAs(first)
  const { title, groups } = useGroups(views, kind)
  const [table, setTable] = useState(kind === "table")
  const tableId = useId()
  const shown = groups.filter((g) => g.items.length)
  if (!shown.length)
    return <EmptyState title={t(shapeOf(first) === "rows" ? "emptyHint" : "noSales")} className="py-6" />
  const tabular = kind !== "table" && kind !== "list"
  return (
    <section aria-label={title} className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-3">
        <h3 className="text-sm font-semibold">{title}</h3>
        {tabular ? (
          <button
            type="button"
            aria-pressed={table}
            aria-controls={tableId}
            onClick={() => setTable((on) => !on)}
            className="min-h-11 shrink-0 rounded-full px-3 text-xs font-medium underline-offset-4 hover:underline focus-visible:ring-2 focus-visible:ring-(--gold) focus-visible:outline-none"
          >
            {table ? t("showChart") : t("showTable")}
          </button>
        ) : null}
      </div>
      {table ? null : (
        <div className="flex flex-col gap-4" data-chart={kind}>
          {shown.map((group) => (
            <div key={group.key} className="flex flex-col gap-2">
              {shown.length > 1 && group.title ? <span className="text-xs text-muted-foreground">{group.title}</span> : null}
              <Drawing kind={kind} group={group} t={t} />
            </div>
          ))}
        </div>
      )}
      <div className={table ? "max-w-full overflow-x-auto" : "sr-only"}>
        <table id={tableId} className={table ? "w-full text-left text-xs tabular-nums" : "sr-only"}>
          <tbody>
            {shown.flatMap((group) =>
              group.items.map((item, i) => (
                <tr key={`${group.key}${i}`} className="border-t border-(--hair)">
                  <th scope="row" className="py-1 pr-2 font-normal">
                    {shown.length > 1 && group.title ? `${group.title} · ` : null}
                    {item.label}
                  </th>
                  <td className="py-1 text-right">{item.shown}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </section>
  )
}

/** Numeric plots of a result at deterministic dimensions for report export. */
export function ReportChart({ views, width, height }: { views: ResultView[]; width: number; height: number }) {
  if (!views.length) return null
  if (views.length > 1 && !views.every((view) => view.kind === "sales"))
    return <>{views.map((view, i) => <ReportChart key={i} views={[view]} width={width} height={height} />)}</>
  return <ReportChartBody views={views} width={width} height={height} />
}

function ReportChartBody({ views, width, height }: { views: ResultView[]; width: number; height: number }) {
  const first = views[0]!
  const kind = drawnAs(first)
  const { title, groups } = useGroups(views, kind)
  if (!["line", "sparkline", "bar", "hbar", "pie", "donut"].includes(kind)) return null
  return (
    <div style={{ width }}>
      {groups.filter((group) => group.items.length).map((group) => (
        <ResultPlot key={group.key} items={group.items} kind={kind as PlotKind} label={[title, group.title].filter(Boolean).join(" · ")} formatValue={group.formatValue} width={width} height={height} exportMode />
      ))}
    </div>
  )
}
