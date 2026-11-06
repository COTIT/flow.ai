import type { Metadata } from "next"
import Link from "next/link"
import { getTranslations } from "next-intl/server"
import { PaletteIcon } from "lucide-react"

import { buttonVariants } from "@genesy/ui/components/button"
import { cn } from "@genesy/ui/lib/utils"

import { PageIntro } from "@/components/page-intro"
import { StoryCard } from "@/components/story-card"
import { photo, type PhotoKey } from "@/lib/photos"
import { card, goldChip, h2, warm } from "@/lib/look"

export async function generateMetadata(): Promise<Metadata> {
  return { title: (await getTranslations("stories"))("tag") }
}

const PEOPLE = ["illundo", "mary", "benard", "nafasi", "rachel"] as const

/** Whose photo AFM has; Rachel has none yet: her card keeps the initials. */
const PHOTO: Partial<Record<(typeof PEOPLE)[number], PhotoKey>> = {
  illundo: "illundo",
  mary: "mary",
  benard: "benard",
  nafasi: "nafasi",
}

export default async function StoriesPage() {
  const [t, tp] = await Promise.all([
    getTranslations("stories"),
    getTranslations("photos"),
  ])
  return (
    <>
      <PageIntro label={t("tag")} title={t("title")} lead={t("lead")} />
      <h2 className="sr-only">{t("tag")}</h2>
      <ul className="grid gap-3 md:grid-cols-2 md:gap-5">
        {PEOPLE.map((k, i) => (
          <StoryCard
            key={k}
            name={t(`${k}.name`)}
            place={t(`${k}.place`)}
            body={t(`${k}.body`)}
            wide={i === 0}
            photo={PHOTO[k] && photo(PHOTO[k], tp(PHOTO[k]))}
          />
        ))}
      </ul>
      <section
        className={cn(
          card,
          warm,
          "items-start gap-3 md:flex-row md:items-center md:p-8"
        )}
      >
        <span className={cn(goldChip, "size-12 rounded-[14px]")}>
          <PaletteIcon aria-hidden className="size-6" />
        </span>
        <div className="flex-1">
          <h2 className={h2}>{t("artTitle")}</h2>
          <p className="text-muted-foreground">{t("artBody")}</p>
        </div>
        <Link
          href="/shop"
          data-slot="button"
          className={buttonVariants({ size: "lg" })}
        >
          {t("artCta")}
        </Link>
      </section>
    </>
  )
}
