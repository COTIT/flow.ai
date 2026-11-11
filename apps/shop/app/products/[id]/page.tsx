import type { Metadata } from "next"
import Link from "next/link"
import { getLocale, getTranslations } from "next-intl/server"
import {
  ArrowLeftIcon,
  ChevronRightIcon,
  MapPinIcon,
  UsersIcon,
} from "lucide-react"

import { Badge } from "@genesy/ui/components/badge"
import { buttonVariants } from "@genesy/ui/components/button"
import { cn } from "@genesy/ui/lib/utils"

import { AddToCart } from "@/components/add-to-cart"
import {
  groupLine,
  LotWarning,
  lotPlace,
  lotStatus,
  MapArea,
} from "@/components/passport"
import { ProductGallery } from "@/components/product-gallery"
import { isSoldOut } from "@/components/stock-badge"
import { card, cardTitle, h2, tag } from "@/lib/look"
import { getLot, getProduct } from "@/lib/storefront"

type Props = { params: Promise<{ id: string }> }

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { id } = await params
  const product = await getProduct(id)
  const description =
    product.short_description ?? product.description?.slice(0, 160) ?? undefined
  return {
    title: product.name,
    description,
    alternates: { canonical: `/products/${product.id}` },
    openGraph: {
      title: product.name,
      description,
      url: `/products/${product.id}`,
    },
  }
}

const crumb =
  "inline-flex min-h-11 min-w-11 items-center gap-1 hover:text-foreground md:min-h-8"

const gold =
  "bg-[image:var(--gold-gradient)] font-semibold text-gold-foreground hover:opacity-90"

/**
 * Product page (bento): breadcrumb, photo, buy tile, "Where it came from" (the batch
 * passport: place, villages and their farmer groups) and About with the labels as chips. The passport card reads the published batch the product links;
 * without one (or when that read fails) the card is left out and the photo spans one row.
 */
export default async function ProductPage({ params }: Props) {
  const { id } = await params
  const [t, tp, tb, locale, product] = await Promise.all([
    getTranslations(),
    getTranslations("passport"),
    getTranslations("batch"),
    getLocale(),
    getProduct(id),
  ])
  // The card is extra: an unpublished code (404) or a failing read only hides it.
  const lot = product.passport
    ? await getLot(product.passport.public_code).catch(() => null)
    : null
  const groups = lot?.farmers ?? []

  return (
    <>
      <nav aria-label={t("product.breadcrumb")}>
        <ol className="flex flex-wrap items-center gap-1 text-[13px] text-muted-foreground">
          <li>
            <Link href="/shop" className={crumb}>
              <ArrowLeftIcon className="size-4" aria-hidden />
              {t("product.back")}
            </Link>
          </li>
          {product.category ? (
            <li className="flex items-center gap-1">
              <ChevronRightIcon aria-hidden className="size-4" />
              <Link
                href={`/shop?category=${product.category.id}`}
                className={crumb}
              >
                {product.category.name}
              </Link>
            </li>
          ) : null}
        </ol>
      </nav>

      {/* The product links its released batch; one recalled or put on hold since says so first. */}
      {lot ? <LotWarning status={lot.status} t={tb} tp={tp} /> : null}

      <section className="grid grid-cols-12 gap-3 md:gap-4">
        <div
          className={cn(
            card,
            "col-span-12 p-3 md:col-span-7 md:p-3",
            lot && "md:row-span-2"
          )}
        >
          <ProductGallery images={product.images ?? []} alt={product.name} />
        </div>
        <div
          className={cn(
            card,
            "col-span-12 gap-[18px] md:col-span-5 md:gap-[18px]"
          )}
        >
          <div>
            {(product.category?.name ?? product.brand) ? (
              <p className={cn(tag, "text-(--tone-warning)")}>
                {product.category?.name ?? product.brand}
              </p>
            ) : null}
            <h1 className="text-[28px] leading-[1.3] font-medium">
              {product.name}
            </h1>
            {product.short_description ? (
              <p className="text-[13px] text-muted-foreground">
                {product.short_description}
              </p>
            ) : null}
          </div>
          <AddToCart product={product} soldOut={isSoldOut(product)} />
        </div>
        {lot ? (
          <section
            aria-labelledby="jar-passport"
            className={cn(card, "col-span-12 md:col-span-5")}
          >
            <div className="flex flex-wrap items-center justify-between gap-3">
              <h2 id="jar-passport" className={cardTitle}>
                {t("product.passport")}
              </h2>
              <Badge variant={lotStatus(lot.status, tp).variant}>
                {lotStatus(lot.status, tp).label}
              </Badge>
            </div>
            {lot.passport.intro ? (
              <p className="line-clamp-3 text-[13px] text-muted-foreground">
                {lot.passport.intro}
              </p>
            ) : null}
            <div className="flex items-center gap-3">
              {lot.passport.map ? (
                <MapArea className="min-h-20 w-[104px] flex-none" />
              ) : null}
              <div className="flex min-w-0 flex-col gap-1">
                <p className="flex items-center gap-1.5 text-[13px] font-semibold">
                  <MapPinIcon aria-hidden className="size-4 shrink-0" />
                  {lotPlace(lot, locale)}
                </p>
                <p className="font-mono text-xs tracking-[0.04em] text-muted-foreground">
                  {t("product.lot", { code: lot.code })}
                </p>
              </div>
            </div>
            {groups.length ? (
              <div className="flex flex-col gap-2">
                <p className="flex items-center gap-1.5 text-[13px] font-semibold">
                  <UsersIcon aria-hidden className="size-4" />
                  {t("product.grownBy")}
                </p>
                <ul className="flex flex-col gap-2">
                  {groups.map((g) => (
                    <li key={g.village} className="flex items-center gap-2.5">
                      <span
                        aria-hidden
                        className="inline-flex size-8 shrink-0 items-center justify-center rounded-full bg-primary text-[11px] font-semibold text-primary-foreground"
                      >
                        {g.village.slice(0, 2).toUpperCase()}
                      </span>
                      <span className="min-w-0 text-[13px]">
                        <span className="font-semibold">{g.village}</span>
                        {groupLine(g, tb, locale) ? (
                          <span className="text-muted-foreground">
                            {" · "}
                            {groupLine(g, tb, locale)}
                          </span>
                        ) : null}
                      </span>
                    </li>
                  ))}
                </ul>
              </div>
            ) : null}
            <Link
              href={`/b/${lot.public_code}`}
              data-slot="button"
              className={cn(buttonVariants(), gold)}
            >
              {t("product.openPassport")}
            </Link>
          </section>
        ) : null}
        <div className={cn(card, "col-span-12")}>
          <div className="gold-rule w-12" />
          <h2 className={h2}>{t("product.about")}</h2>
          <p className="max-w-[760px] whitespace-pre-line text-muted-foreground">
            {product.description ?? t("product.noDescription")}
          </p>
          <Link
            href="/stories"
            className="inline-flex min-h-11 w-fit items-center gap-1.5 font-medium underline-offset-4 hover:underline md:min-h-8"
          >
            {t("product.people")}
            <ChevronRightIcon aria-hidden className="size-4" />
          </Link>
          {product.labels.length ? (
            <div className="flex flex-wrap gap-2">
              {product.labels.map((label) => (
                <Badge key={label} variant="slate" data-dot="false">
                  {label}
                </Badge>
              ))}
            </div>
          ) : null}
        </div>
      </section>
    </>
  )
}
