// Plain <img>, not next/image: signed file links change on every fetch, and next/image
// would cache each one under its own URL. The app's CSP img-src allows the file host.
import { AlertTriangleIcon, ExternalLinkIcon, FileTextIcon } from "lucide-react"

import type {
  MapPoint,
  PublicBlock,
  PublicLot,
  PublicPassport,
} from "@genesy/api-client/model"
import { formatDay, formatShare } from "@genesy/ui/lib/format"
import { cn } from "@genesy/ui/lib/utils"

type Props = {
  /** The published passport, as the public and storefront lot replies carry it. */
  passport: PublicPassport
  /** The batch around it: code, status, dates, product, consented farmers. */
  lot: Omit<PublicLot, "passport" | "code">
  locale: string
  /** Translator for the `batch` messages. Method syntax so next-intl's typed `t` fits. */
  t(key: string, values?: Record<string, string | number>): string
}

/**
 * A batch passport as the public sees it (shop `/b/{code}`, office preview). Display only:
 * data fetching and link freshness stay with the app. Every section hides when empty.
 * `t` reads the shared `batch` messages from @genesy/i18n.
 */
export function PassportView({ passport, lot, locale, t }: Props) {
  const day = (d: string) => formatDay(d, locale)
  const country = passport.origin
    ? new Intl.DisplayNames(locale, { type: "region" }).of(
        passport.origin.country
      )
    : null
  const facts = [
    lot.produced_on && [t("produced"), day(lot.produced_on)],
    lot.best_before && [t("bestBefore"), day(lot.best_before)],
    lot.expires_on && [t("expires"), day(lot.expires_on)],
    lot.origin && [t("origin"), lot.origin],
  ].filter(Boolean) as [string, string][]

  return (
    <article className="flex flex-col gap-10">
      {lot.status !== "released" ? (
        <div
          role={lot.status === "recalled" ? "alert" : "status"}
          className={cn(
            "flex items-start gap-3 rounded-lg border p-4 font-medium",
            lot.status === "recalled"
              ? "border-destructive bg-destructive/10 text-destructive"
              : "border-gold bg-gold/15"
          )}
        >
          <AlertTriangleIcon className="mt-0.5 size-5 shrink-0" aria-hidden />
          <p>{lot.status === "recalled" ? t("recalled") : t("quarantine")}</p>
        </div>
      ) : null}

      <header className="flex flex-col gap-4">
        {passport.banner ? (
          <img
            src={passport.banner.url}
            alt=""
            referrerPolicy="no-referrer"
            className="aspect-[2/1] w-full rounded-lg bg-muted object-cover md:aspect-[3/1]"
          />
        ) : null}
        <p className="tag text-muted-foreground">
          {[lot.product.brand, t("code", { code: lot.public_code })]
            .filter(Boolean)
            .join(" · ")}
        </p>
        <h1 className="heading-1">{passport.title ?? lot.product.name}</h1>
        {passport.title ? (
          <p className="heading-2 text-muted-foreground">{lot.product.name}</p>
        ) : null}
        {passport.intro ? (
          <p className="max-w-3xl text-lg whitespace-pre-line">
            {passport.intro}
          </p>
        ) : null}
        {facts.length ? (
          <dl className="grid grid-cols-2 gap-4 rounded-lg bg-muted p-4 sm:grid-cols-4">
            {facts.map(([label, value]) => (
              <div key={label} className="flex flex-col gap-1">
                <dt className="tag text-muted-foreground">{label}</dt>
                <dd className="font-medium">{value}</dd>
              </div>
            ))}
          </dl>
        ) : null}
      </header>

      {passport.origin || passport.map ? (
        <Section title={t("where")}>
          {passport.origin ? (
            <p className="text-lg">
              {[passport.origin.place, passport.origin.region, country]
                .filter(Boolean)
                .join(", ")}
            </p>
          ) : null}
          {passport.map ? <MapArea map={passport.map} t={t} /> : null}
        </Section>
      ) : null}

      {passport.story.length ? (
        <Section title={t("story")}>
          <div className="flex flex-col gap-8">
            {passport.story.map((block, i) => (
              <StoryBlock key={i} block={block} />
            ))}
          </div>
        </Section>
      ) : null}

      {passport.materials.length ? (
        <Section title={t("materials")}>
          <ul className="divide-y rounded-lg border">
            {passport.materials.map((m) => (
              <li
                key={m.name}
                className="flex items-center justify-between gap-4 px-4 py-3"
              >
                <span>
                  <span className="font-medium">{m.name}</span>
                  {m.origin ? (
                    <span className="text-muted-foreground"> · {m.origin}</span>
                  ) : null}
                </span>
                {m.share_bp != null ? (
                  <span className="tabular-nums">
                    {formatShare(m.share_bp, locale)}
                  </span>
                ) : null}
              </li>
            ))}
          </ul>
        </Section>
      ) : null}

      {passport.certificates.length ? (
        <Section title={t("certificates")}>
          <ul className="grid gap-4 sm:grid-cols-2">
            {passport.certificates.map((c) => (
              <li
                key={c.name}
                className="flex flex-col gap-1 rounded-lg border p-4"
              >
                <p className="font-medium">{c.name}</p>
                <p className="text-sm text-muted-foreground">
                  {[
                    c.issuer && t("issuer", { issuer: c.issuer }),
                    c.number && t("number", { number: c.number }),
                    c.valid_until &&
                      t("validUntil", { date: day(c.valid_until) }),
                  ]
                    .filter(Boolean)
                    .join(" · ")}
                </p>
                {c.file ? (
                  <a
                    href={c.file.url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="mt-1 flex w-fit items-center gap-1 text-sm underline underline-offset-4"
                  >
                    <FileTextIcon className="size-4" aria-hidden />
                    {t("certFile")}
                  </a>
                ) : null}
              </li>
            ))}
          </ul>
        </Section>
      ) : null}

      {lot.farmers.length ? (
        <Section title={t("farmers")}>
          <ul className="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-4">
            {lot.farmers.map((f, i) => (
              <li key={i} className="flex flex-col gap-2">
                {f.photo ? (
                  <img
                    src={f.photo.url}
                    alt={f.first_name}
                    loading="lazy"
                    referrerPolicy="no-referrer"
                    className="aspect-square w-full rounded-lg bg-muted object-cover"
                  />
                ) : null}
                <p className="font-medium">{f.first_name}</p>
                <p className="text-sm text-muted-foreground">{f.village}</p>
                {f.harvest_dates.length ? (
                  <p className="text-sm text-muted-foreground">
                    {t("harvested", {
                      dates: f.harvest_dates.map(day).join(", "),
                    })}
                  </p>
                ) : null}
              </li>
            ))}
          </ul>
        </Section>
      ) : null}

      <p className="text-xs text-muted-foreground">
        {t("published", {
          version: passport.version,
          date: day(passport.published_at.slice(0, 10)),
        })}
      </p>
    </article>
  )
}

function Section({
  title,
  children,
}: {
  title: string
  children: React.ReactNode
}) {
  return (
    <section className="flex flex-col gap-4">
      <h2 className="heading-2">{title}</h2>
      <div className="gold-rule w-16" />
      {children}
    </section>
  )
}

function StoryBlock({ block }: { block: PublicBlock }) {
  switch (block.kind) {
    case "text":
      return (
        <div className="flex max-w-3xl flex-col gap-2">
          {block.title ? <h3 className="heading-3">{block.title}</h3> : null}
          <p className="whitespace-pre-line">{block.body}</p>
        </div>
      )
    case "image":
      return block.image ? (
        <figure className="flex flex-col gap-2">
          <img
            src={block.image.url}
            alt={block.caption ?? ""}
            loading="lazy"
            referrerPolicy="no-referrer"
            className="w-full rounded-lg bg-muted object-cover"
          />
          {block.caption ? (
            <figcaption className="text-sm text-muted-foreground">
              {block.caption}
            </figcaption>
          ) : null}
        </figure>
      ) : null
    case "facts":
      return (
        <dl className="grid grid-cols-2 gap-4 sm:grid-cols-3">
          {block.items.map((f) => (
            <div
              key={f.label}
              className="flex flex-col gap-1 border-l-2 border-gold pl-3"
            >
              <dt className="tag text-muted-foreground">{f.label}</dt>
              <dd className="text-2xl font-medium">{f.value}</dd>
            </div>
          ))}
        </dl>
      )
    case "quote":
      return (
        <blockquote className="max-w-3xl border-l-2 border-gold pl-4">
          <p className="text-lg italic">“{block.text}”</p>
          {block.by ? (
            <footer className="mt-1 text-sm text-muted-foreground">
              {block.by}
            </footer>
          ) : null}
        </blockquote>
      )
  }
}

/**
 * Where the batch comes from, as a soft area rather than a pin: the API rounds the point
 * to ~1 km and no map tiles are loaded (no third party sees the visitor). The outbound
 * link opens OpenStreetMap at area zoom, without a marker.
 */
function MapArea({ map, t }: { map: MapPoint; t: Props["t"] }) {
  const lat = `${Math.abs(map.lat).toFixed(2)}° ${map.lat < 0 ? "S" : "N"}`
  const lng = `${Math.abs(map.lng).toFixed(2)}° ${map.lng < 0 ? "W" : "E"}`
  return (
    <figure className="flex flex-col gap-3">
      <div className="relative overflow-hidden rounded-lg border bg-muted">
        <svg viewBox="0 0 600 260" className="block h-auto w-full" aria-hidden>
          <defs>
            <pattern
              id="passport-grid"
              width="30"
              height="30"
              patternUnits="userSpaceOnUse"
            >
              <path d="M30 0H0V30" fill="none" className="stroke-border" />
            </pattern>
            <radialGradient id="passport-area">
              <stop
                offset="0"
                className="[stop-color:var(--teal)]"
                stopOpacity="0.55"
              />
              <stop
                offset="0.6"
                className="[stop-color:var(--teal)]"
                stopOpacity="0.25"
              />
              <stop
                offset="1"
                className="[stop-color:var(--teal)]"
                stopOpacity="0"
              />
            </radialGradient>
          </defs>
          <rect width="600" height="260" fill="url(#passport-grid)" />
          <circle cx="300" cy="130" r="110" fill="url(#passport-area)" />
          <circle
            cx="300"
            cy="130"
            r="70"
            fill="none"
            className="stroke-teal"
            strokeDasharray="4 6"
          />
        </svg>
        <div className="absolute inset-x-0 bottom-0 flex flex-wrap items-end justify-between gap-2 p-3">
          {map.label ? (
            <span className="rounded-sm bg-background/90 px-2 py-1 text-sm font-medium">
              {map.label}
            </span>
          ) : (
            <span />
          )}
          <span className="rounded-sm bg-background/90 px-2 py-1 text-xs tabular-nums">
            {lat}, {lng}
          </span>
        </div>
      </div>
      <figcaption className="flex flex-col gap-2 text-sm text-muted-foreground sm:flex-row sm:items-center sm:justify-between">
        <span>{t("mapNote")}</span>
        <a
          href={`https://www.openstreetmap.org/#map=11/${map.lat}/${map.lng}`}
          target="_blank"
          rel="noopener noreferrer"
          className="flex w-fit shrink-0 items-center gap-1 text-foreground underline underline-offset-4"
        >
          {t("mapLink")}
          <ExternalLinkIcon className="size-4" aria-hidden />
        </a>
      </figcaption>
    </figure>
  )
}
