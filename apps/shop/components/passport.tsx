// Plain <img>, not next/image: signed file links change on every fetch (see passport-view).
import {
  ExternalLinkIcon,
  ShieldCheckIcon,
  TriangleAlertIcon,
} from "lucide-react"

import type {
  PublicBlock,
  PublicFarmerGroup,
  PublicLot,
} from "@genesy/api-client/model"
import { Badge } from "@genesy/ui/components/badge"
import { buttonVariants } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { formatDay, formatShare } from "@genesy/ui/lib/format"
import { cn } from "@genesy/ui/lib/utils"

import { card, cardTitle, h2, tag, tile } from "@/lib/look"

import { FadeIn } from "./motion"

type T = (key: string, values?: Record<string, string | number>) => string

/** The batch's status badge; `tp` reads the shop's `passport` messages. */
export const lotStatus = (status: string, tp: T) =>
  status === "recalled"
    ? { variant: "red" as const, label: tp("status.recalled") }
    : status === "quarantine"
      ? { variant: "gold" as const, label: tp("status.quarantine") }
      : { variant: "teal" as const, label: tp("status.released") }

/** "Place, region, country" from the passport's origin, else the batch's own origin line. */
export function lotPlace(
  lot: Pick<PublicLot, "origin" | "passport">,
  locale: string
) {
  const origin = lot.passport.origin
  if (!origin) return lot.origin
  const country = new Intl.DisplayNames(locale, { type: "region" }).of(
    origin.country
  )
  return [origin.place, origin.region, country].filter(Boolean).join(", ")
}

/** A recalled batch: the red banner; on hold: a warning; released: nothing. Goes first. */
export function LotWarning({ status, t, tp }: { status: string; t: T; tp: T }) {
  return (
    <>
      {status === "recalled" ? (
        <div
          role="alert"
          className="flex items-start gap-3.5 rounded-[20px] bg-[#b42318] px-5 py-[18px] text-white dark:bg-[#7a1a12]"
        >
          <TriangleAlertIcon aria-hidden className="size-7 shrink-0" />
          <div>
            <p className="text-lg font-bold">{tp("recalledTitle")}</p>
            <p className="opacity-92">{tp("recalledBody")}</p>
          </div>
        </div>
      ) : status === "quarantine" ? (
        <Notice variant="warning" title={t("quarantine")} />
      ) : null}
    </>
  )
}

/** "12 farmers · Harvested 1 Jul – 20 Jul": the API's own count (null below 3) and window. */
export function groupLine(g: PublicFarmerGroup, t: T, locale: string) {
  const day = (d: string) => formatDay(d, locale)
  return [
    // null below 3 farmers, for privacy: then no count at all.
    g.farmers !== null && t("farmerCount", { count: g.farmers }),
    g.harvest_start && g.harvest_end
      ? t("harvested", { start: day(g.harvest_start), end: day(g.harvest_end) })
      : g.harvest_start || g.harvest_end
        ? t("harvestedOn", { date: day((g.harvest_start ?? g.harvest_end)!) })
        : null,
  ]
    .filter(Boolean)
    .join(" · ")
}

/**
 * Mock `ShopPassport` (+ `ShopPassportRecalled`): a batch passport as tiles. Display only;
 * every tile hides when its data is empty, and a tile whose partner is hidden takes the full
 * row. The recall banner (or the on-hold warning) comes first. `t` reads the shared `batch`
 * messages, `tp` the shop's own `passport` ones.
 */
export function Passport({
  lot,
  locale,
  t,
  tp,
}: {
  lot: Omit<PublicLot, "code">
  locale: string
  t: T
  tp: T
}) {
  const { passport } = lot
  const day = (d: string) => formatDay(d, locale)
  const dates = [
    lot.produced_on && `${t("produced")} ${day(lot.produced_on)}`,
    lot.best_before && `${t("bestBefore")} ${day(lot.best_before)}`,
    lot.expires_on && `${t("expires")} ${day(lot.expires_on)}`,
  ].filter(Boolean)
  const place = lotPlace(lot, locale)
  const facts = passport.story.flatMap((b) =>
    b.kind === "facts" ? b.items : []
  )
  const quotes = passport.story.filter((b) => b.kind === "quote")
  const story = passport.story.filter(
    (b) => b.kind === "text" || (b.kind === "image" && b.image)
  )
  const status = lotStatus(lot.status, tp)

  const hasWhere = Boolean(place || passport.map)
  const hasInside = passport.materials.length > 0 || facts.length > 0
  const hasQuote = quotes.length > 0
  const hasStory = story.length > 0
  /** 7 + 5 when both tiles of a row show, the full row when one is alone. */
  const span = (wide: boolean, partner: boolean) =>
    !partner ? "md:col-span-12" : wide ? "md:col-span-7" : "md:col-span-5"

  return (
    <>
      <LotWarning status={lot.status} t={t} tp={tp} />

      <div className="grid grid-cols-12 gap-3 md:gap-4">
        <header
          className={cn(
            card,
            "col-span-12 gap-0 overflow-hidden p-0 md:gap-0 md:p-0"
          )}
        >
          {passport.banner ? (
            <img
              src={passport.banner.url}
              alt=""
              referrerPolicy="no-referrer"
              className="block h-[280px] w-full object-cover"
            />
          ) : null}
          <div className="flex flex-wrap items-start justify-between gap-4 px-7 py-6">
            <div className="flex min-w-0 flex-[1_1_520px] flex-col gap-2">
              <p className={cn(tag, "text-(--tone-warning)")}>
                {[lot.product.name, t("code", { code: lot.public_code })].join(
                  " · "
                )}
              </p>
              <h1 className="text-[32px] leading-[1.2] font-medium tracking-[0.04em] uppercase">
                {passport.title ?? lot.product.name}
              </h1>
              {passport.intro ? (
                <p className="max-w-[680px] whitespace-pre-line text-muted-foreground">
                  {passport.intro}
                </p>
              ) : null}
            </div>
            <div className="flex flex-col items-end gap-2">
              <Badge variant={status.variant}>{status.label}</Badge>
              {dates.length ? (
                <p className="text-xs text-muted-foreground">
                  {dates.join(" · ")}
                </p>
              ) : null}
            </div>
          </div>
        </header>

        {hasWhere ? (
          <FadeIn
            aria-labelledby="passport-where"
            className={cn(card, "col-span-12", span(true, hasInside))}
          >
            <div className="flex flex-wrap items-center justify-between gap-3">
              <h2 id="passport-where" className={cardTitle}>
                {t("where")}
              </h2>
              {passport.map ? (
                <a
                  href={`https://www.openstreetmap.org/#map=11/${passport.map.lat}/${passport.map.lng}`}
                  target="_blank"
                  rel="noopener noreferrer"
                  data-slot="button"
                  className={buttonVariants({ variant: "ghost", size: "sm" })}
                >
                  {t("mapLink")}
                  <ExternalLinkIcon aria-hidden />
                </a>
              ) : null}
            </div>
            {place ? <p className="font-semibold">{place}</p> : null}
            {passport.map ? (
              <>
                <MapArea label={passport.map.label} />
                <p className="text-xs text-muted-foreground">{t("mapNote")}</p>
              </>
            ) : null}
          </FadeIn>
        ) : null}

        {hasInside ? (
          <FadeIn
            aria-labelledby="passport-inside"
            className={cn(card, "col-span-12", span(false, hasWhere))}
          >
            <h2 id="passport-inside" className={cardTitle}>
              {t("materials")}
            </h2>
            {passport.materials.map((m) => (
              <div key={m.name} className="flex flex-col gap-2">
                <div className="flex justify-between gap-3 text-[13px]">
                  <span>
                    {m.name}
                    {m.origin ? `, ${m.origin}` : ""}
                  </span>
                  {m.share_bp != null ? (
                    <span className="font-semibold tabular-nums">
                      {formatShare(m.share_bp, locale)}
                    </span>
                  ) : null}
                </div>
                {m.share_bp != null ? (
                  <div className="h-2 overflow-hidden rounded-full bg-(--hair)">
                    <i
                      className="block h-full rounded-full bg-teal"
                      style={{ width: `${m.share_bp / 100}%` }}
                    />
                  </div>
                ) : null}
              </div>
            ))}
            {facts.length ? (
              <dl className="grid grid-cols-1 gap-2.5 md:grid-cols-2">
                {facts.map((f) => (
                  <div key={f.label} className={tile}>
                    <dt className="text-xs text-muted-foreground">{f.label}</dt>
                    <dd className="font-semibold">{f.value}</dd>
                  </div>
                ))}
              </dl>
            ) : null}
          </FadeIn>
        ) : null}

        {lot.farmers.length ? (
          <FadeIn
            aria-labelledby="passport-farmers"
            className={cn(card, "col-span-12")}
          >
            <h2 id="passport-farmers" className={h2}>
              {t("farmers")}
            </h2>
            {/* Groups per village only: the API never sends names, photos, kg or prices. */}
            <ul className="grid grid-cols-1 gap-3 md:grid-cols-3 md:gap-5">
              {lot.farmers.map((g) => (
                <li
                  key={g.village}
                  className={cn(tile, "flex items-center gap-3.5")}
                >
                  <span
                    aria-hidden
                    className="inline-flex size-14 shrink-0 items-center justify-center rounded-full bg-primary text-lg font-semibold text-primary-foreground"
                  >
                    {g.village.slice(0, 2).toUpperCase()}
                  </span>
                  <div className="min-w-0">
                    <p className="font-semibold">{g.village}</p>
                    <p className="text-[13px] text-muted-foreground">
                      {groupLine(g, t, locale)}
                    </p>
                  </div>
                </li>
              ))}
            </ul>
          </FadeIn>
        ) : null}

        {hasQuote ? (
          <FadeIn
            as="div"
            className={cn(
              card,
              "col-span-12 justify-center",
              span(false, hasStory)
            )}
          >
            <p className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase">
              {tp("inTheirWords")}
            </p>
            {quotes.map((q, i) =>
              q.kind === "quote" ? (
                <blockquote key={i} className="flex flex-col gap-3">
                  <p className="text-xl leading-normal italic">“{q.text}”</p>
                  {q.by ? (
                    <footer className="text-[13px] text-muted-foreground">
                      — {q.by}
                    </footer>
                  ) : null}
                </blockquote>
              ) : null
            )}
          </FadeIn>
        ) : null}

        {hasStory ? (
          <FadeIn
            aria-labelledby="passport-story"
            className={cn(card, "col-span-12", span(true, hasQuote))}
          >
            <h2 id="passport-story" className={cardTitle}>
              {t("story")}
            </h2>
            {story.map((block, i) => (
              <StoryBlock key={i} block={block} />
            ))}
          </FadeIn>
        ) : null}

        {passport.certificates.length ? (
          <FadeIn
            aria-labelledby="passport-certificates"
            className={cn(card, "col-span-12")}
          >
            <h2 id="passport-certificates" className={cardTitle}>
              {t("certificates")}
            </h2>
            <ul className="grid grid-cols-1 gap-3 md:grid-cols-2 md:gap-4">
              {passport.certificates.map((c) => (
                <li
                  key={c.name}
                  className={cn(tile, "flex items-center gap-3")}
                >
                  <span className="inline-flex size-9 shrink-0 items-center justify-center rounded-[12px] bg-(--tint-success) text-(--tone-success)">
                    <ShieldCheckIcon aria-hidden className="size-[18px]" />
                  </span>
                  <div className="min-w-0 flex-auto">
                    <p className="text-[13px] font-semibold">{c.name}</p>
                    <p className="text-xs text-muted-foreground">
                      {[
                        c.issuer && t("issuer", { issuer: c.issuer }),
                        c.number && t("number", { number: c.number }),
                        c.valid_until &&
                          t("validUntil", { date: day(c.valid_until) }),
                      ]
                        .filter(Boolean)
                        .join(" · ")}
                    </p>
                  </div>
                  {c.file ? (
                    <a
                      href={c.file.url}
                      target="_blank"
                      rel="noopener noreferrer"
                      data-slot="button"
                      className={buttonVariants({
                        variant: "outline",
                        size: "sm",
                      })}
                    >
                      {t("certFile")}
                    </a>
                  ) : null}
                </li>
              ))}
            </ul>
          </FadeIn>
        ) : null}
      </div>

      <p className="text-xs text-muted-foreground">
        {t("published", {
          version: passport.version,
          date: day(passport.published_at.slice(0, 10)),
        })}
      </p>
    </>
  )
}

function StoryBlock({ block }: { block: PublicBlock }) {
  if (block.kind === "text")
    return (
      <div className="flex flex-col gap-1">
        {block.title ? <h3 className="font-semibold">{block.title}</h3> : null}
        <p className="text-[13px] whitespace-pre-line text-muted-foreground">
          {block.body}
        </p>
      </div>
    )
  if (block.kind === "image" && block.image)
    return (
      <figure className="flex flex-col gap-2">
        <img
          src={block.image.url}
          alt={block.caption ?? ""}
          loading="lazy"
          referrerPolicy="no-referrer"
          className="h-40 w-full rounded-[16px] object-cover"
        />
        {block.caption ? (
          <figcaption className="text-xs text-muted-foreground">
            {block.caption}
          </figcaption>
        ) : null}
      </figure>
    )
  return null
}

/**
 * Mock `.map`: where the batch comes from as a soft area on a grid, not a pin. The API rounds
 * the point to ~1 km and no map tiles load (no third party sees the visitor).
 */
export function MapArea({
  label,
  className,
}: {
  label?: string | null
  className?: string
}) {
  return (
    <div
      className={cn(
        "relative min-h-[260px] overflow-hidden rounded-[16px] bg-(--glass-subtle) bg-[radial-gradient(circle_at_58%_46%,rgb(1_144_155/30%)_0_60px,rgb(1_144_155/12%)_61px_96px,transparent_97px),repeating-linear-gradient(0deg,var(--hair)_0_1px,transparent_1px_28px),repeating-linear-gradient(90deg,var(--hair)_0_1px,transparent_1px_28px)]",
        className
      )}
    >
      {label ? (
        <span className="absolute top-[46%] left-[58%] -translate-x-1/2 -translate-y-1/2 rounded-full border border-(--glass-border) bg-(--glass-strong) px-3 py-1.5 text-xs font-semibold whitespace-nowrap">
          {label}
        </span>
      ) : null}
    </div>
  )
}
