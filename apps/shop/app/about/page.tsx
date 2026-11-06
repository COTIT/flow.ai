import type { Metadata } from "next"
import Link from "next/link"
import { getLocale, getTranslations } from "next-intl/server"
import {
  CalendarDaysIcon,
  HandshakeIcon,
  QrCodeIcon,
  SproutIcon,
} from "lucide-react"

import { buttonVariants } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { cn } from "@genesy/ui/lib/utils"

import { PageIntro } from "@/components/page-intro"
import { upcoming } from "@/lib/events"
import { PHOTOS } from "@/lib/photos"
import { card, goldChip, h2, prose, tile } from "@/lib/look"

export async function generateMetadata(): Promise<Metadata> {
  return { title: (await getTranslations("about"))("tag") }
}

const day = (iso: string, locale: string) =>
  new Intl.DateTimeFormat(locale, {
    day: "numeric",
    month: "long",
    timeZone: "UTC",
  }).format(new Date(`${iso}T00:00:00Z`))

export default async function AboutPage() {
  const [t, locale] = await Promise.all([getTranslations("about"), getLocale()])
  const events = upcoming(new Date().toISOString().slice(0, 10))
  return (
    <>
      <PageIntro label={t("tag")} title={t("title")} lead={t("lead")} />

      <section className="grid gap-3 md:grid-cols-3 md:gap-5">
        {(["mission", "vision", "problem"] as const).map((k) => (
          <article key={k} className={card}>
            <h2 className={h2}>{t(`${k}Title`)}</h2>
            <p className={cn(prose, "text-muted-foreground")}>
              {t(`${k}Body`)}
            </p>
          </article>
        ))}
      </section>

      <section className={cn(card, "gap-6 md:flex-row md:gap-8 md:p-10")}>
        <div className="flex flex-1 flex-col gap-3">
          <img
            src={PHOTOS.farmers.src}
            alt={(await getTranslations("photos"))("farmers")}
            width={PHOTOS.farmers.width}
            height={PHOTOS.farmers.height}
            loading="lazy"
            decoding="async"
            className="aspect-[3/2] w-full rounded-[16px] object-cover"
          />
          <span className={cn(goldChip, "size-12 rounded-[14px]")}>
            <SproutIcon aria-hidden className="size-6" />
          </span>
          <h2 className={h2}>{t("villageTitle")}</h2>
          <p className={prose}>{t("villageBody")}</p>
        </div>
        <div className={cn(tile, "flex flex-1 flex-col gap-3")}>
          <span className={cn(goldChip, "size-12 rounded-[14px]")}>
            <QrCodeIcon aria-hidden className="size-6" />
          </span>
          <h2 className={h2}>{t("traceTitle")}</h2>
          <p className={prose}>{t("traceBody")}</p>
        </div>
      </section>

      <section className="grid gap-3 md:grid-cols-2 md:gap-5">
        {(["soul", "team"] as const).map((k) => (
          <article key={k} className={card}>
            <h2 className={h2}>{t(`${k}Title`)}</h2>
            <p className={cn(prose, "text-muted-foreground")}>
              {t(`${k}Body`)}
            </p>
          </article>
        ))}
      </section>

      <section aria-labelledby="events" className={card}>
        <h2 id="events" className={h2}>
          {t("eventsTitle")}
        </h2>
        <p className="text-muted-foreground">{t("eventsLead")}</p>
        {events.length ? (
          <ul className="grid gap-3 md:grid-cols-2">
            {events.map((e) => (
              <li key={e.name} className={cn(tile, "flex items-start gap-3")}>
                <CalendarDaysIcon
                  aria-hidden
                  className="mt-0.5 size-5 shrink-0 text-(--tone-warning)"
                />
                <div className="min-w-0">
                  <p className="font-semibold">{e.name}</p>
                  <p className="text-[13px] text-muted-foreground">
                    {e.start === e.end
                      ? day(e.start, locale)
                      : `${day(e.start, locale)} – ${day(e.end, locale)}`}
                    {" · "}
                    {e.place}
                  </p>
                </div>
              </li>
            ))}
          </ul>
        ) : (
          <Notice variant="info" title={t("eventsNone")}>
            {t("eventsNoneWhy")}
          </Notice>
        )}
      </section>

      <section
        className={cn(card, "items-start gap-3 md:flex-row md:items-center")}
      >
        <span className={cn(goldChip, "size-12 rounded-[14px]")}>
          <HandshakeIcon aria-hidden className="size-6" />
        </span>
        <div className="flex-1">
          <h2 className={h2}>{t("supplierTitle")}</h2>
          <p className="text-muted-foreground">{t("supplierBody")}</p>
        </div>
        <Link
          href="/suppliers"
          data-slot="button"
          className={buttonVariants({ size: "lg" })}
        >
          {t("supplierCta")}
        </Link>
      </section>
    </>
  )
}
