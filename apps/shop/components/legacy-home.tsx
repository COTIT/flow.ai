import Link from "next/link"
import { getLocale, getTranslations } from "next-intl/server"
import {
  ArrowRightIcon,
  PackageCheckIcon,
  QrCodeIcon,
  SproutIcon,
  UsersIcon,
} from "lucide-react"

import { buttonVariants } from "@verzo/ui/components/button"
import { formatMoney } from "@verzo/ui/lib/money"
import { cn } from "@verzo/ui/lib/utils"

import { FadeIn } from "@/components/motion"
import { ProductCard } from "@/components/product-card"
import { ProductImage } from "@/components/product-image"
import { isSoldOut } from "@/components/stock-badge"
import { StoryCard } from "@/components/story-card"
import { TraceForm } from "@/components/trace-form"
import { card, goldChip, h2, prose, tag } from "@/lib/look"
import { shownPrice } from "@/lib/price"
import { photo, PHOTOS } from "@/lib/photos"
import { listCategories, listProducts } from "@/lib/storefront"

const sectionHead = "flex flex-wrap items-end justify-between gap-3"
const more =
  "inline-flex min-h-11 items-center gap-1.5 font-medium underline-offset-4 hover:underline"

/** Home: featured product, live catalogue, trace lookup and farmer stories. */
export default async function LegacyHomePage() {
  const [t, locale, categories, first] = await Promise.all([
    getTranslations(),
    getLocale(),
    listCategories(),
    listProducts(1),
  ])
  const available = first.data.filter((product) => !isSoldOut(product))
  const featured = available.slice(0, 8)
  const hero = available.find((product) => product.images?.length)
  const labels = {
    inStock: t("product.inStock"),
    low: t("product.low"),
    soldOut: t("product.soldOut"),
    imageSoon: t("product.imageSoon"),
  }
  const promises = [
    ["passport", QrCodeIcon, "/b"],
    ["stock", PackageCheckIcon, "/shop"],
    ["farmers", UsersIcon, "/stories"],
  ] as const

  return (
    <>
      <section className="afm-hero">
        <div className="afm-hero-copy">
          <p className={cn(tag, "afm-hero-tag")}>{t("home.tag")}</p>
          <h1>{t("home.title")}</h1>
          <p className={prose}>{t("home.lead")}</p>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
            <Link
              href="/shop"
              data-slot="button"
              className={cn(buttonVariants({ size: "lg" }), "afm-hero-cta")}
            >
              {t("content.shopCta")}
            </Link>
            <Link href="/stories" className={more}>
              {t("home.stories")}
              <ArrowRightIcon aria-hidden className="size-4" />
            </Link>
          </div>
        </div>
        <div className="afm-hero-media">
          {hero ? (
            <>
              <ProductImage
                image={hero.images?.[0]}
                alt={hero.name}
                label={labels.imageSoon}
                className="afm-hero-image"
                fetchPriority="high"
              />
              <div className="afm-hero-product">
                {(hero.category?.name ?? hero.brand) ? (
                  <p className={tag}>{hero.category?.name ?? hero.brand}</p>
                ) : null}
                <h2>{hero.name}</h2>
                <div className="flex flex-wrap items-center gap-3">
                  <span className="text-lg font-semibold tabular-nums">
                    {formatMoney(
                      shownPrice(hero),
                      hero.currency_decimals,
                      hero.currency,
                      locale
                    )}
                  </span>
                  <Link
                    href={`/products/${hero.id}`}
                    data-slot="button"
                    className={buttonVariants({ size: "lg" })}
                  >
                    {t("home.viewProduct")}
                  </Link>
                </div>
              </div>
            </>
          ) : (
            <img
              src={PHOTOS.landscape.src}
              alt={t("photos.landscape")}
              width={PHOTOS.landscape.width}
              height={PHOTOS.landscape.height}
              fetchPriority="high"
              className="afm-hero-image"
            />
          )}
        </div>
      </section>

      <ul className="afm-trust-strip">
        {promises.map(([key, Icon, href]) => (
          <li key={key}>
            <Link href={href}>
              <Icon aria-hidden className="size-5 shrink-0" />
              {t(`home.${key}Title`)}
            </Link>
          </li>
        ))}
      </ul>

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
          <ul className="afm-product-grid grid grid-cols-2 gap-3 md:grid-cols-4 md:gap-5">
            {featured.map((p) => (
              <li key={p.id} className="flex min-w-0">
                <ProductCard product={p} locale={locale} labels={labels} />
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {categories.length ? (
        <section aria-labelledby="categories" className="flex flex-col gap-4">
          <h2 id="categories" className={h2}>
            {t("home.categoriesTitle")}
          </h2>
          <ul className="grid grid-cols-2 gap-3 md:grid-cols-5 md:gap-4">
            {categories.map((c) => (
              <li key={c.id} className="flex min-w-0">
                <Link
                  href={`/shop?category=${c.id}`}
                  className="afm-category flex w-full flex-col justify-between gap-2 rounded-[16px] p-3 outline-none focus-visible:ring-3 focus-visible:ring-ring/60 md:p-4"
                >
                  <span className="text-[17px] leading-tight font-semibold">
                    {c.name}
                  </span>
                  <span className="flex flex-wrap items-center justify-between text-[13px]">
                    {t("home.categoryCount", { count: c.product_count })}
                    <ArrowRightIcon aria-hidden className="size-4" />
                  </span>
                </Link>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      <TraceForm />

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

      <FadeIn
        aria-labelledby="people"
        className="flex flex-col gap-4"
        duration={0.32}
      >
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
              photo={photo(k, t(`photos.${k}`))}
            />
          ))}
        </ul>
      </FadeIn>

      <section
        className={cn(
          card,
          "afm-mission gap-4 md:flex-row md:items-center md:gap-8 md:p-10"
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
