"use client"

import { useId, useState } from "react"
import { useLocale, useTranslations } from "next-intl"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { formatDate, formatShare } from "@genesy/ui/lib/format"
import { formatMoney } from "@genesy/ui/lib/money"
import { fieldName } from "../../lib/helpers"
import { drawnAs, shapeOf, type Kind, type ResultView } from "../../lib/views"
import { Series } from "./series"

/** One drawn value: `value` is the API's number (drawing scale only), `shown` its formatted text. */
type Item = { label: string; value: number | null; shown: string }
type Group = { key: string; title: string | null; items: Item[]; change?: string | null }

const signed = (bp: number, locale: string) => `${bp > 0 ? "+" : bp < 0 ? "−" : ""}${formatShare(Math.abs(bp), locale)}`

/** Any result as labelled groups (one per currency, section or list), every figure as the API sent it. */
function useGroups(views: ResultView[], kind: Kind): { title: string; groups: Group[] } {
  const t = useTranslations("voice.results")
  const locale = useLocale()
  const day = (iso: string) => formatDate(iso.slice(0, 10), locale)
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
          items: c.rows.map((r) => ({ label: r.label ?? t("breakdown.walkIn"), value: r.gross, shown: money(r.gross, c.decimals, c.currency) })),
        })),
      }
    case "lot_origin":
      return {
        title: view.title ?? t("originTitle"),
        groups: [{
          key: "materials",
          title: null,
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

/** Slices sized by the API's amounts: drawing scale only, no share is computed or printed. */
function Slices({ items, donut, label }: { items: Item[]; donut: boolean; label: string }) {
  const parts = items.map((i) => Math.max(i.value ?? 0, 0))
  const total = parts.reduce((n, v) => n + v, 0)
  if (!total) return null
  const ends = parts.map((_, i) => parts.slice(0, i + 1).reduce((n, v) => n + v, 0) / total)
  const point = (turn: number) =>
    `${50 + 48 * Math.sin(turn * 2 * Math.PI)},${50 - 48 * Math.cos(turn * 2 * Math.PI)}`
  return (
    <svg role="img" aria-label={label} viewBox="0 0 100 100" className="size-40 shrink-0 text-(--gold)">
      {parts.map((part, i) => {
        if (!part) return null
        const share = part / total
        const from = ends[i]! - share
        const d =
          share > 0.9999
            ? "M50,2 A48,48 0 1 1 50,98 A48,48 0 1 1 50,2 Z"
            : `M50,50 L${point(from)} A48,48 0 ${share > 0.5 ? 1 : 0} 1 ${point(ends[i]!)} Z`
        return <path key={i} d={d} fill="currentColor" opacity={shade(i)} className="stroke-card" strokeWidth="0.5" />
      })}
      {donut ? <circle cx="50" cy="50" r="28" className="fill-card" /> : null}
    </svg>
  )
}

function HBars({ items }: { items: Item[] }) {
  const top = Math.max(...items.map((i) => i.value ?? 0), 1)
  return (
    <ul className="flex flex-col gap-2">
      {items.map((item, i) => (
        <li key={i} className="flex flex-col gap-1">
          <span className="flex flex-wrap items-baseline justify-between gap-x-3 text-xs">
            <span className="min-w-0 truncate">{item.label}</span>
            <span className="font-semibold tabular-nums">{item.shown}</span>
          </span>
          <svg aria-hidden viewBox="0 0 100 8" preserveAspectRatio="none" className="h-2.5 w-full">
            <rect width="100" height="8" rx="4" className="fill-(--hair)" />
            {item.value && item.value > 0 ? (
              <rect width={Math.max((item.value / top) * 100, 2)} height="8" rx="4" className="fill-(--gold)" />
            ) : null}
          </svg>
        </li>
      ))}
    </ul>
  )
}

function Legend({ items, swatch }: { items: Item[]; swatch: boolean }) {
  return (
    <ul className="flex min-w-0 flex-1 flex-col gap-1 text-xs">
      {items.map((item, i) => (
        <li key={i} className="flex items-baseline gap-2">
          {swatch ? <span aria-hidden className="size-2.5 shrink-0 rounded-full bg-(--gold)" style={{ opacity: shade(i) }} /> : null}
          <span className="min-w-0 truncate">{item.label}</span>
          <span className="ml-auto font-semibold tabular-nums">{item.shown}</span>
        </li>
      ))}
    </ul>
  )
}

function Drawing({ kind, group, t }: { kind: Kind; group: Group; t: ReturnType<typeof useTranslations> }) {
  const { items } = group
  const named = items.map((i) => `${i.label}: ${i.shown}`).join("; ")
  switch (kind) {
    case "line":
    case "sparkline":
    case "bar":
      return (
        <div className="flex flex-col gap-1">
          <Series line={kind !== "bar"} values={items.map((i) => i.value ?? 0)} className={kind === "sparkline" ? "h-10" : "h-28"} />
          <div className="flex justify-between gap-3 text-xs text-muted-foreground">
            <span>{items[0]?.label}</span>
            <span>{items.length > 1 ? items.at(-1)?.label : null}</span>
          </div>
        </div>
      )
    case "hbar":
      return <HBars items={items} />
    case "pie":
    case "donut":
      return (
        <div className="flex flex-wrap items-center gap-4">
          <Slices items={items} donut={kind === "donut"} label={named} />
          <Legend items={items} swatch />
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
                <span className="min-w-0 truncate">{item.label}</span>
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
    </section>
  )
}
