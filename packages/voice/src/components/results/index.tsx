"use client"

import { createContext, useContext, type ReactNode } from "react"
import { useLocale, useTranslations } from "next-intl"
import { ChevronRight, ExternalLink } from "lucide-react"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { formatDate, formatQty, formatShare } from "@genesy/ui/lib/format"
import { formatMoney } from "@genesy/ui/lib/money"
import { dailyAsSales, type Money, type ResultView, drawnAs } from "../../lib/views"
import { fieldName, formatPrice } from "../../lib/helpers"
import { useVoice } from "../provider"
import { SalesChart } from "./sales-chart"
import { KindChart } from "./kinds"

type Of<K extends ResultView["kind"]> = Extract<ResultView, { kind: K }>
/** A host's own words for a record status; `domain` is the record kind, e.g. "orders". */
export type StatusLabel = (domain: string | undefined, status: string) => string
const HostStatus = createContext<StatusLabel | undefined>(undefined)

const CARD =
  "w-full max-w-[36rem] min-w-0 self-start rounded-2xl rounded-bl-md border border-(--hair) bg-card p-4 text-sm"
const humanize = fieldName

function useLabels() {
  const t = useTranslations("voice")
  const all = useTranslations()
  const locale = useLocale()
  const host = useContext(HostStatus)
  const { resolveHref } = useVoice()
  return {
    t,
    locale,
    money: (m: Money | null) => (m ? formatMoney(m.minor, m.decimals, m.currency, locale) : null),
    // The record page's own status words first, so the chat says what Office says.
    status: (s: string | null, domain?: string) =>
      !s ? null
      : host ? host(domain, s)
      : domain && all.has(`${domain}.statuses.${s}`) ? all(`${domain}.statuses.${s}`)
      : t.has(`values.${s}`) ? t(`values.${s}`)
      : humanize(s),
    day: (iso: string | null) => (iso ? formatDate(iso, locale) : null),
    href: resolveHref,
  }
}

/** One row: a link to its Office page (new tab, like record links), or plain text without one. */
function Row({ href: raw, label, detail, end }: { href: string | null; label: string; detail?: ReactNode; end?: ReactNode }) {
  const { resolveHref } = useVoice()
  // A host may refuse a destination (""): the row stays, without a link.
  const href = raw ? resolveHref(raw) : ""
  const body = (
    <>
      <span className="flex min-w-0 flex-col">
        <span className="truncate font-medium">{label}</span>
        {detail ? <span className="truncate text-xs text-muted-foreground">{detail}</span> : null}
      </span>
      <span className="ml-auto flex shrink-0 items-center gap-2 text-right tabular-nums">
        {end}
        {href ? <ChevronRight aria-hidden className="size-4 text-muted-foreground" /> : null}
      </span>
    </>
  )
  const cls = "flex min-h-11 items-center gap-3 rounded-xl px-2 py-1.5"
  return (
    <li>
      {href ? (
        <a
          href={href}
          target="_blank"
          rel="noopener noreferrer"
          className={`${cls} transition-colors duration-100 hover:bg-(--hair) focus-visible:ring-2 focus-visible:ring-(--gold) focus-visible:outline-none motion-reduce:transition-none`}
        >
          {body}
        </a>
      ) : (
        <div className={cls}>{body}</div>
      )}
    </li>
  )
}

function Records({ view }: { view: Of<"records"> }) {
  const { t, locale, money, status, href } = useLabels()
  const title = t(`results.resources.${view.resource}`)
  if (!view.rows.length)
    return <EmptyState title={t(`results.empty.${view.resource}`)} description={t("results.emptyHint")} className="py-6" />
  return (
    <section aria-label={title} className="flex flex-col gap-2">
      <h3 className="text-sm font-semibold">
        {title} <span className="font-normal text-muted-foreground tabular-nums">· {view.total}</span>
      </h3>
      <ul className="-mx-2 flex flex-col">
        {view.rows.map((row) => (
          <Row
            key={row.id}
            href={row.href}
            label={row.label}
            detail={[row.detail, status(row.status, view.resource)].filter(Boolean).join(" · ")}
            end={
              money(row.money) ??
              (row.stock !== null
                ? t("results.inStock", { count: new Intl.NumberFormat(locale).format(row.stock) })
                : null)
            }
          />
        ))}
      </ul>
      {view.total > view.rows.length && href(view.href) ? (
        <p className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
          {t("results.showing", { shown: view.rows.length, total: view.total })}
          <a
            href={href(view.href)}
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex min-h-11 items-center gap-1 font-medium text-foreground underline-offset-4 hover:underline focus-visible:ring-2 focus-visible:ring-(--gold) focus-visible:outline-none"
          >
            {t("results.openAll")}
            <ExternalLink aria-hidden className="size-3.5" />
          </a>
        </p>
      ) : null}
    </section>
  )
}

function LotOrigin({ view }: { view: Of<"lot_origin"> }) {
  const { t, locale } = useLabels()
  if (!view.origin && !view.materials.length)
    return <EmptyState title={t("results.noOrigin")} description={t("results.noOriginHint")} className="py-6" />
  const country = view.origin && new Intl.DisplayNames(locale, { type: "region" }).of(view.origin.country)
  return (
    <section aria-label={t("results.originTitle")} className="flex flex-col gap-3">
      <h3 className="text-sm font-semibold">{view.title ?? t("results.originTitle")}</h3>
      {view.origin ? (
        <p className="text-base font-medium">
          {[view.origin.place, view.origin.region, country].filter(Boolean).join(", ")}
        </p>
      ) : null}
      {view.materials.length ? (
        <div className="flex flex-col gap-1">
          <h4 className="text-xs text-muted-foreground">{t("results.materials")}</h4>
          <ul className="flex flex-col gap-1">
            {view.materials.map((m) => (
              <li key={m.name} className="flex items-baseline justify-between gap-3">
                <span>
                  {m.name}
                  {m.origin ? <span className="text-muted-foreground"> · {m.origin}</span> : null}
                </span>
                {m.share !== null ? <span className="tabular-nums">{formatShare(m.share, locale)}</span> : null}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </section>
  )
}

function Trail({ view }: { view: Of<"trail"> }) {
  const { t, status, day } = useLabels()
  if (!view.events.length)
    return <EmptyState title={t("results.noTrail")} description={t("results.noTrailHint")} className="py-6" />
  return (
    <section aria-label={t("results.trailTitle")} className="flex flex-col gap-3">
      <h3 className="text-sm font-semibold">{t("results.trailTitle")}</h3>
      <ol className="flex flex-col">
        {view.events.map((e, i) => (
          <li key={`${e.at}${i}`} className="relative flex gap-3 pb-3 last:pb-0">
            <span aria-hidden className="relative flex w-3 shrink-0 justify-center">
              <span className="mt-1.5 size-2.5 rounded-full bg-(--gold)" />
              {i < view.events.length - 1 ? <span className="absolute top-4 bottom-0 w-px bg-(--hair)" /> : null}
            </span>
            <span className="flex min-w-0 flex-col">
              <span className="font-medium">
                {t.has(`results.events.${e.kind}`) ? t(`results.events.${e.kind}`) : humanize(e.kind)}
                {e.to ? `: ${[status(e.from, "lots"), status(e.to, "lots")].filter(Boolean).join(" → ")}` : null}
              </span>
              <span className="text-xs text-muted-foreground">
                {[day(e.at), e.by ? t("results.by", { name: e.by }) : null].filter(Boolean).join(" · ")}
              </span>
            </span>
          </li>
        ))}
      </ol>
    </section>
  )
}

function Attention({ view }: { view: Of<"attention"> }) {
  const { t, money, status, day } = useLabels()
  const sections = view.sections.filter((s) => s.items.length || Object.values(s.counts).some(Boolean))
  if (!sections.length)
    return <EmptyState title={t("results.allClear")} description={t("results.allClearHint")} className="py-6" />
  return (
    <section aria-label={t("results.attentionTitle")} className="flex flex-col gap-4">
      <h3 className="text-sm font-semibold">{t("results.attentionTitle")}</h3>
      {sections.map((s) => (
        <section key={s.key} aria-label={t(`results.sections.${s.key}`)} className="flex flex-col gap-1">
          <h4 className="flex flex-wrap items-baseline gap-x-3 text-xs">
            <span className="font-semibold">{t(`results.sections.${s.key}`)}</span>
            {Object.entries(s.counts).map(([key, count]) => (
              <span key={key} className="text-muted-foreground tabular-nums">
                {t.has(`results.counts.${key}`) ? t(`results.counts.${key}`) : humanize(key)} {count}
              </span>
            ))}
          </h4>
          {s.items.length ? (
            <ul className="-mx-2 flex flex-col">
              {s.items.map((item, i) => (
                <Row
                  key={`${item.label}${i}`}
                  href={item.href}
                  label={item.label}
                  detail={[
                    item.detail,
                    status(item.status, s.key),
                    item.attempts !== null ? t("results.attempts", { count: item.attempts }) : null,
                    s.key === "till_shifts" && item.at ? t("results.opened", { date: day(item.at)! }) : day(item.at),
                  ]
                    .filter(Boolean)
                    .join(" · ")}
                  end={money(item.money) ?? (item.level !== null ? t("results.level", { level: item.level }) : null)}
                />
              ))}
            </ul>
          ) : null}
        </section>
      ))}
    </section>
  )
}

/** Stock per product (grouped per warehouse) or one product's stock per bin: the API's quantities as written. */
function Stock({ view }: { view: Of<"stock"> }) {
  const { t, locale, href } = useLabels()
  const title = t(view.scope === "list" ? "results.stock.title" : "results.stock.perBin")
  const shown = view.groups.reduce((n, g) => n + g.rows.length, 0)
  if (!shown) return <EmptyState title={t("results.stock.empty")} description={t("results.emptyHint")} className="py-6" />
  const more = view.href && view.total !== null && view.total > shown ? href(view.href) : ""
  return (
    <section aria-label={title} className="flex flex-col gap-3">
      <h3 className="text-sm font-semibold">
        {title}
        {view.total !== null ? <span className="font-normal text-muted-foreground tabular-nums"> · {view.total}</span> : null}
      </h3>
      {view.groups.map((group, g) => {
        const where = group.title ?? t("results.stock.noWarehouse")
        return (
          <section key={`${where}${g}`} aria-label={where} className="flex flex-col gap-1">
            <h4 className="text-xs font-semibold text-muted-foreground">{where}</h4>
            <ul className="-mx-2 flex flex-col">
              {group.rows.map((row, i) => (
                <Row
                  key={i}
                  href={row.href}
                  label={row.label ?? t("results.stock.noBin")}
                  detail={
                    <>
                      {row.detail}
                      {/* Its own line that wraps: a narrow screen never cuts the reserved or minimum figure. */}
                      <span className="block whitespace-normal">
                        {[
                          t("results.stock.reserved", { count: formatQty(row.reserved, locale) }),
                          row.minimum !== null ? t("results.stock.minimum", { count: row.minimum }) : null,
                        ]
                          .filter(Boolean)
                          .join(" · ")}
                      </span>
                    </>
                  }
                  end={
                    <span className="flex flex-col items-end leading-tight">
                      <span className="text-base font-semibold">{formatQty(row.available ?? row.level, locale)}</span>
                      <span className="text-xs text-muted-foreground">
                        {t(row.available !== null ? "results.stock.available" : "results.stock.inStock")}
                      </span>
                    </span>
                  }
                />
              ))}
            </ul>
          </section>
        )
      })}
      {more ? (
        <p className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
          {t("results.showing", { shown, total: view.total! })}
          <a
            href={more}
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex min-h-11 items-center gap-1 font-medium text-foreground underline-offset-4 hover:underline focus-visible:ring-2 focus-visible:ring-(--gold) focus-visible:outline-none"
          >
            {t("results.openAll")}
            <ExternalLink aria-hidden className="size-3.5" />
          </a>
        </p>
      ) : null}
    </section>
  )
}

/** Quick search hits per kind, as the API ranked them; a group with no hits is left out. */
function Search({ view }: { view: Of<"search"> }) {
  const { t, money, status } = useLabels()
  const groups = view.groups.filter((g) => g.rows.length)
  if (!groups.length) return <EmptyState title={t("results.search.none")} description={t("results.emptyHint")} className="py-6" />
  return (
    <section aria-label={t("results.search.title")} className="flex flex-col gap-3">
      <h3 className="text-sm font-semibold">{t("results.search.title")}</h3>
      {groups.map((group) => (
        <section key={group.key} aria-label={t(`results.search.${group.key}`)} className="flex flex-col gap-1">
          <h4 className="text-xs font-semibold text-muted-foreground">
            {t(`results.search.${group.key}`)} <span className="font-normal tabular-nums">· {group.rows.length}</span>
          </h4>
          <ul className="-mx-2 flex flex-col">
            {group.rows.map((hit, i) => (
              <Row
                key={i}
                href={hit.href}
                label={hit.label}
                detail={[hit.detail, status(hit.status, group.key)].filter(Boolean).join(" · ")}
                end={money(hit.money)}
              />
            ))}
          </ul>
        </section>
      ))}
    </section>
  )
}

/** One record: headline, key facts, its lines and the API's own totals. Nothing is added up. */
function Document({ view }: { view: Of<"document"> }) {
  const { t, locale, money, status, day, href } = useLabels()
  const title = view.title ?? t("results.document.draft")
  const link = href(view.href)
  const lines =
    view.lineMoney === "excl" ? "results.document.linesExcl"
    : view.lineMoney === "incl" ? "results.document.linesIncl"
    : "results.document.stock"
  return (
    <section aria-label={`${t(`results.resources.${view.resource}`)} ${title}`} className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="text-base font-semibold">{title}</h3>
        {view.status ? (
          <span className="rounded-full bg-(--hair) px-2.5 py-0.5 text-xs font-medium">{status(view.status, view.resource)}</span>
        ) : null}
      </div>
      {view.facts.length ? (
        <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1 text-xs">
          {view.facts.map((fact) => (
            <div key={fact.key} className="contents">
              <dt className="text-muted-foreground">{t(`results.facts.${fact.key}`)}</dt>
              <dd className="min-w-0 truncate text-right font-medium tabular-nums">{"text" in fact ? fact.text : "price" in fact ? formatPrice(fact.price, fact.decimals, fact.currency, locale) : day(fact.date)}</dd>
            </div>
          ))}
        </dl>
      ) : null}
      {view.lines.length ? (
        <div className="flex flex-col">
          <h4 className="text-xs font-semibold text-muted-foreground">{t(lines)}</h4>
          <ul className="flex flex-col">
            {view.lines.map((line, i) => (
              <li key={i} className="flex min-h-11 items-center gap-3 border-b border-(--hair) py-1.5 last:border-b-0">
                <span className="min-w-0 flex-1 truncate">{line.label ?? t("results.stock.noBin")}</span>
                {line.quantity ? (
                  <span className="shrink-0 text-muted-foreground tabular-nums">
                    {view.lineMoney ? `× ${formatQty(line.quantity, locale)}` : formatQty(line.quantity, locale)}
                  </span>
                ) : null}
                {line.money ? <span className="shrink-0 font-medium tabular-nums">{money(line.money)}</span> : null}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
      {view.totals.length ? (
        <dl className="flex flex-col gap-1 border-t border-(--hair) pt-2">
          {view.totals.map((total, i) => (
            <div
              key={total.key}
              className={`flex items-baseline justify-between gap-3 ${i === view.totals.length - 1 ? "text-base font-semibold" : "text-xs text-muted-foreground"}`}
            >
              <dt>{t(`results.totals.${total.key}`)}</dt>
              <dd className="tabular-nums">{money(total.money)}</dd>
            </div>
          ))}
        </dl>
      ) : null}
      {link ? (
        <a
          href={link}
          target="_blank"
          rel="noopener noreferrer"
          className="inline-flex min-h-11 items-center gap-1 self-start text-xs font-medium underline-offset-4 hover:underline focus-visible:ring-2 focus-visible:ring-(--gold) focus-visible:outline-none"
        >
          {t("results.document.open")}
          <ExternalLink aria-hidden className="size-3.5" />
        </a>
      ) : null}
    </section>
  )
}

/** Date and time, in the viewer's clock: when a sale was rung up or a drawer opened. */
function useWhen() {
  const locale = useLocale()
  return (iso: string) =>
    new Intl.DateTimeFormat(locale, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" }).format(new Date(iso))
}

/** A short list with its API total and an "open all" link to the Office page. */
function Listed({ title, total, shown, href: raw, children }: { title: string; total: number; shown: number; href: string; children: ReactNode }) {
  const { t, href } = useLabels()
  const more = total > shown ? href(raw) : ""
  return (
    <section aria-label={title} className="flex flex-col gap-2">
      <h3 className="text-sm font-semibold">
        {title} <span className="font-normal text-muted-foreground tabular-nums">· {total}</span>
      </h3>
      <ul className="-mx-2 flex flex-col">{children}</ul>
      {more ? (
        <p className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
          {t("results.showing", { shown, total })}
          <a
            href={more}
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex min-h-11 items-center gap-1 font-medium text-foreground underline-offset-4 hover:underline focus-visible:ring-2 focus-visible:ring-(--gold) focus-visible:outline-none"
          >
            {t("results.openAll")}
            <ExternalLink aria-hidden className="size-3.5" />
          </a>
        </p>
      ) : null}
    </section>
  )
}

/** Till sales as booked: number, time, how it was paid, its total when the reply has the currency. */
function TillSales({ view }: { view: Of<"till_sales"> }) {
  const { t, money } = useLabels()
  const when = useWhen()
  if (!view.rows.length) return <EmptyState title={t("results.till.noSales")} description={t("results.emptyHint")} className="py-6" />
  return (
    <Listed title={t("results.till.sales")} total={view.total} shown={view.rows.length} href={view.href}>
      {view.rows.map((sale, i) => (
        <Row
          key={i}
          href={sale.href}
          label={sale.label}
          detail={
            <>
              {[when(sale.at), sale.method ? (t.has(`results.till.methods.${sale.method}`) ? t(`results.till.methods.${sale.method}`) : humanize(sale.method)) : null]
                .filter(Boolean)
                .join(" · ")}
              {sale.mismatch ? <span className="block whitespace-normal">{t("results.till.mismatch")}</span> : null}
            </>
          }
          end={money(sale.money)}
        />
      ))}
    </Listed>
  )
}

/** Drawer shifts: open with the cash expected so far, closed with what was counted. */
function TillShifts({ view }: { view: Of<"till_shifts"> }) {
  const { t, money } = useLabels()
  const when = useWhen()
  if (!view.rows.length) return <EmptyState title={t("results.till.noShifts")} description={t("results.emptyHint")} className="py-6" />
  return (
    <Listed title={t("results.till.shifts")} total={view.total} shown={view.rows.length} href={view.href}>
      {view.rows.map((shift, i) => {
        const open = shift.state === "open"
        const amount = open ? shift.expected : shift.counted
        return (
          <Row
            key={i}
            href={null}
            label={t("results.till.opened", { at: when(shift.opened) })}
            detail={
              <>
                {[t(`results.till.state.${shift.state}`), shift.difference && shift.state !== "even" ? money(shift.difference) : null]
                  .filter(Boolean)
                  .join(" · ")}
                {shift.closed ? <span className="block whitespace-normal">{t("results.till.closed", { at: when(shift.closed) })}</span> : null}
              </>
            }
            end={
              amount ? (
                <span className="flex flex-col items-end leading-tight">
                  <span className="font-semibold">{money(amount)}</span>
                  <span className="text-xs text-muted-foreground">{t(open ? "results.till.expected" : "results.till.counted")}</span>
                </span>
              ) : null
            }
          />
        )
      })}
    </Listed>
  )
}

/** A result picture in the chat. Unknown kinds render nothing (the agent's words still answer). */
export function VoiceResult({ views, statusLabel }: { views: ResultView[]; statusLabel?: StatusLabel }) {
  const first = views[0]
  if (!first) return null
  // An asked-for kind, and pictures with no special drawing of their own, use the shared kinds.
  const body =
    first.presentation || first.kind === "comparison" || first.kind === "breakdown" ? <KindChart views={views} />
    : first.kind === "sales" ? <SalesChart views={views.filter((v): v is Of<"sales"> => v.kind === "sales")} />
    : first.kind === "sales_daily" ? <SalesChart views={dailyAsSales(first)} trend />
    : first.kind === "records" ? <Records view={first} />
    : first.kind === "lot_origin" ? <LotOrigin view={first} />
    : first.kind === "trail" ? <Trail view={first} />
    : first.kind === "attention" ? <Attention view={first} />
    : first.kind === "stock" ? <Stock view={first} />
    : first.kind === "till_sales" ? <TillSales view={first} />
    : first.kind === "till_shifts" ? <TillShifts view={first} />
    : first.kind === "search" ? <Search view={first} />
    : first.kind === "document" ? <Document view={first} />
    : null
  return body ? (
    <HostStatus.Provider value={statusLabel}>
      <div key={drawnAs(first)} data-role="result" data-kind={first.kind} className={CARD}>{body}</div>
    </HostStatus.Provider>
  ) : null
}
