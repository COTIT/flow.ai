"use client"

import { createContext, useContext, type ReactNode } from "react"
import { useLocale, useTranslations } from "next-intl"
import { ChevronRight, ExternalLink } from "lucide-react"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { formatDate, formatShare } from "@genesy/ui/lib/format"
import { formatMoney } from "@genesy/ui/lib/money"
import type { Money, ResultView } from "../../lib/views"
import { fieldName } from "../../lib/helpers"
import { useVoice } from "../provider"
import { SalesChart } from "./sales-chart"

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
function Row({ href, label, detail, end }: { href: string | null; label: string; detail?: ReactNode; end?: ReactNode }) {
  const { resolveHref } = useVoice()
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
          href={resolveHref(href)}
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
      {view.total > view.rows.length ? (
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

/** A result picture in the chat. Unknown kinds render nothing (the agent's words still answer). */
export function VoiceResult({ views, statusLabel }: { views: ResultView[]; statusLabel?: StatusLabel }) {
  const first = views[0]
  if (!first) return null
  const body =
    first.kind === "sales" ? <SalesChart views={views.filter((v): v is Of<"sales"> => v.kind === "sales")} />
    : first.kind === "records" ? <Records view={first} />
    : first.kind === "lot_origin" ? <LotOrigin view={first} />
    : first.kind === "trail" ? <Trail view={first} />
    : first.kind === "attention" ? <Attention view={first} />
    : null
  return body ? (
    <HostStatus.Provider value={statusLabel}>
      <div data-role="result" data-kind={first.kind} className={CARD}>{body}</div>
    </HostStatus.Provider>
  ) : null
}
