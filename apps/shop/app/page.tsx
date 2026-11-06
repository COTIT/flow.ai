import Link from "next/link"
import { redirect } from "next/navigation"
import { getLocale, getTranslations } from "next-intl/server"
import {
  ArrowRightIcon,
  PackageCheckIcon,
  QrCodeIcon,
  SproutIcon,
  UsersIcon,
} from "lucide-react"

import { buttonVariants } from "@genesy/ui/components/button"
import { cn } from "@genesy/ui/lib/utils"

import { Headline } from "@/components/motion"
import { ProductCard } from "@/components/product-card"
import { isSoldOut } from "@/components/stock-badge"
import { StoryCard } from "@/components/story-card"
import { TraceForm } from "@/components/trace-form"
import { card, goldChip, h2, prose, tag, warm } from "@/lib/look"
import { listCategories, listProducts } from "@/lib/storefront"

/** Category tiles take turns through AFM's earth tones (white text passes AA on each). */
const TONES = ["#7a4b2a", "#5f6b3a", "#9a4a24", "#3d5a5b", "#6b4e2e", "#4f5d2f"]

const sectionHead = "flex flex-wrap items-end justify-between gap-3"
const more =
  "inline-flex min-h-11 items-center gap-1.5 font-medium underline-offset-4 hover:underline"

/** Home: mission and trace box, what Genesy lets you check, categories, stock, people. */
export default async function HomePage({
  searchParams,
}: {
  searchParams: Promise<{ page?: string }>
}) {
  // The catalogue used to live here: keep old page links working.
  const { page } = await searchParams
  if (page) redirect(`/shop?page=${encodeURIComponent(page)}`)

  const [t, locale, categories, first] = await Promise.all([
    getTranslations(),
    getLocale(),
    listCategories(),
    listProducts(1),
  ])
  const featured = first.data.filter((p) => !isSoldOut(p)).slice(0, 8)
  const labels = {
    inStock: t("product.inStock"),
    low: t("product.low"),
    soldOut: t("product.soldOut"),
    imageSoon: t("product.imageSoon"),
  }
  const promises = [
    ["passport", QrCodeIcon],
    ["stock", PackageCheckIcon],
    ["farmers", UsersIcon],
  ] as const

  return (
    <>
      <section
        className={cn(
          card,
          warm,
          "gap-6 p-6 md:grid md:grid-cols-[1.35fr_1fr] md:items-center md:gap-10 md:p-12"
        )}
      >
        <div className="flex min-w-0 flex-col gap-4">
          <p className={cn(tag, "text-(--tone-warning)")}>{t("home.tag")}</p>
          <h1 className="text-[32px] leading-[1.1] font-medium md:text-[52px]">
            <Headline text={t("home.title")} />
          </h1>
          <p className={cn(prose, "text-muted-foreground md:text-[17px]")}>
            {t("home.lead")}
          </p>
          <div className="flex flex-wrap gap-2">
            <Link
              href="/shop"
              data-slot="button"
              className={buttonVariants({ size: "lg" })}
            >
              {t("content.shopCta")}
            </Link>
            <Link
              href="/stories"
              data-slot="button"
              className={buttonVariants({ size: "lg", variant: "outline" })}
            >
              {t("home.stories")}
            </Link>
          </div>
        </div>
        <TraceForm className="glass-strong" />
      </section>

      <section aria-labelledby="promises" className="flex flex-col gap-4">
        <h2 id="promises" className={h2}>
          {t("home.promisesTitle")}
        </h2>
        <ul className="grid gap-3 md:grid-cols-3 md:gap-5">
          {promises.map(([k, Icon]) => (
            <li key={k} className={cn(card, "flex-row items-start gap-3")}>
              <span className={cn(goldChip, "size-11 rounded-[14px]")}>
                <Icon aria-hidden className="size-5" />
              </span>
              <div className="min-w-0">
                <h3 className="font-semibold">{t(`home.${k}Title`)}</h3>
                <p className="text-[13px] text-muted-foreground">
                  {t(`home.${k}Body`)}
                </p>
              </div>
            </li>
          ))}
        </ul>
      </section>

      {categories.length ? (
        <section aria-labelledby="categories" className="flex flex-col gap-4">
          <h2 id="categories" className={h2}>
            {t("home.categoriesTitle")}
          </h2>
          <ul className="grid grid-cols-2 gap-3 md:grid-cols-5 md:gap-4">
            {categories.map((c, i) => (
              <li key={c.id} className="flex min-w-0">
                <Link
                  href={`/shop?category=${c.id}`}
                  style={{ backgroundColor: TONES[i % TONES.length] }}
                  className="flex min-h-[112px] w-full flex-col justify-between gap-3 rounded-[18px] p-4 text-white outline-none hover:brightness-110 focus-visible:ring-3 focus-visible:ring-ring/60 md:min-h-[132px]"
                >
                  <span className="text-[17px] leading-tight font-semibold">
                    {c.name}
                  </span>
                  <span className="flex items-center justify-between text-[13px] text-white/85">
                    {t("home.categoryCount", { count: c.product_count })}
                    <ArrowRightIcon aria-hidden className="size-4" />
                  </span>
                </Link>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {featured.length ? (
        <section aria-labelledby="featured" className="flex flex-col gap-4">
          <div className={sectionHead}>
            <h2 id="featured" className={h2}>
              {t("home.featuredTitle")}
            </h2>
            <Link href="/shop" className={more}>
              {t("home.featuredAll")}
              <ArrowRightIcon aria-hidden className="size-4" />
            </Link>
          </div>
          <ul className="grid grid-cols-2 gap-3 md:grid-cols-4 md:gap-5">
            {featured.map((p) => (
              <li key={p.id} className="flex min-w-0">
                <ProductCard product={p} locale={locale} labels={labels} />
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      <section aria-labelledby="people" className="flex flex-col gap-4">
        <div className={sectionHead}>
          <h2 id="people" className={h2}>
            {t("home.storiesTitle")}
          </h2>
          <Link href="/stories" className={more}>
            {t("home.storiesAll")}
            <ArrowRightIcon aria-hidden className="size-4" />
          </Link>
        </div>
        <ul className="grid gap-3 md:grid-cols-3 md:gap-5">
          {(["illundo", "mary", "benard"] as const).map((k) => (
            <StoryCard
              key={k}
              name={t(`stories.${k}.name`)}
              place={t(`stories.${k}.place`)}
              body={t(`stories.${k}.body`)}
            />
          ))}
        </ul>
      </section>

      <section
        className={cn(
          card,
          "gap-4 md:flex-row md:items-center md:gap-8 md:p-10"
        )}
      >
        <span className={cn(goldChip, "size-14 rounded-[16px]")}>
          <SproutIcon aria-hidden className="size-7" />
        </span>
        <div className="flex flex-1 flex-col gap-2">
          <h2 className={h2}>{t("about.villageTitle")}</h2>
          <p className={cn(prose, "text-muted-foreground")}>
            {t("about.villageBody")}
          </p>
        </div>
        <Link
          href="/about"
          data-slot="button"
          className={cn(
            buttonVariants({ variant: "outline", size: "lg" }),
            "shrink-0"
          )}
        >
          {t("home.missionCta")}
        </Link>
      </section>
    </>
  )
}
