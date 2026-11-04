import type { Metadata } from "next"
import Link from "next/link"
import { getLocale, getTranslations } from "next-intl/server"
import { ArrowLeftIcon } from "lucide-react"

import { Badge } from "@genesy/ui/components/badge"
import { buttonVariants } from "@genesy/ui/components/button"
import { cn } from "@genesy/ui/lib/utils"

import { AddToCart } from "@/components/add-to-cart"
import { lotPlace, lotStatus, MapArea } from "@/components/passport"
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

const gold =
  "bg-[image:var(--gold-gradient)] font-semibold text-[#0a183a] hover:opacity-90"

/**
 * Mock `ShopProduct` (bento): photo, buy tile, "This jar's passport" and About with the
 * product's labels as chips. The passport card reads the published batch the product links;
 * without one (or when that read fails) the card is left out and the photo spans one row.
 */
export default async function ProductPage({ params }: Props) {
  const { id } = await params
  const [t, tp, locale, product] = await Promise.all([
    getTranslations(),
    getTranslations("passport"),
    getLocale(),
    getProduct(id),
  ])
  // The card is extra: an unpublished code (404) or a failing read only hides it.
  const lot = product.passport
    ? await getLot(product.passport.public_code).catch(() => null)
    : null
  const groups = lot?.farmers ?? []
  // The API's own counts only: one village's count, else how many villages.
  const farmers =
    groups.length === 1 && groups[0]!.farmers !== null
      ? t("batch.farmerCount", { count: groups[0]!.farmers })
      : groups.length > 1
        ? t("product.villages", { count: groups.length })
        : null

  return (
    <>
      <Link
        href="/"
        className="flex min-h-11 w-fit items-center gap-1 text-[13px] text-muted-foreground hover:text-foreground md:min-h-0"
      >
        <ArrowLeftIcon className="size-4" aria-hidden />
        {t("product.back")}
      </Link>

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
            {product.brand ? (
              <p className={cn(tag, "text-(--tone-warning)")}>
                {product.brand}
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
            <div className="flex items-center gap-3">
              {lot.passport.map ? (
                <MapArea className="min-h-24 w-[120px] flex-none" />
              ) : null}
              <div className="flex min-w-0 flex-col gap-1">
                <p className="text-[13px] font-semibold">
                  {lotPlace(lot, locale)}
                </p>
                <p className="font-mono text-xs tracking-[0.04em] text-muted-foreground">
                  {t("product.lot", { code: lot.code })}
                </p>
                {groups.length ? (
                  <div className="flex flex-wrap items-center gap-1">
                    {groups.slice(0, 3).map((g) => (
                      <span
                        key={g.village}
                        title={g.village}
                        aria-hidden
                        className="inline-flex size-7 shrink-0 items-center justify-center rounded-full bg-primary text-[11px] font-semibold text-primary-foreground"
                      >
                        {g.village.slice(0, 2).toUpperCase()}
                      </span>
                    ))}
                    {farmers ? (
                      <span className="text-xs text-muted-foreground">
                        {farmers}
                      </span>
                    ) : null}
                  </div>
                ) : null}
              </div>
            </div>
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
