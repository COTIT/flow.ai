import type { Metadata } from "next"
import Link from "next/link"
import { getLocale, getTranslations } from "next-intl/server"
import { ArrowLeftIcon } from "lucide-react"

import { formatMoney } from "@genesy/ui/lib/money"

import { AddToCart } from "@/components/add-to-cart"
import { ProductImage } from "@/components/product-image"
import { isSoldOut, StockBadge } from "@/components/stock-badge"
import { getProduct } from "@/lib/storefront"

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

export default async function ProductPage({ params }: Props) {
  const { id } = await params
  const [t, locale, product] = await Promise.all([
    getTranslations(),
    getLocale(),
    getProduct(id),
  ])
  const money = (minor: number) =>
    formatMoney(minor, product.currency_decimals, product.currency, locale)

  return (
    <div className="flex flex-col gap-6">
      <Link
        href="/"
        className="flex w-fit items-center gap-1 text-sm text-muted-foreground hover:text-foreground hover:underline"
      >
        <ArrowLeftIcon className="size-4" aria-hidden />
        {t("product.back")}
      </Link>

      <article className="grid gap-6 md:grid-cols-2 md:gap-10">
        <ProductImage label={t("product.imageSoon")} className="rounded-lg" />

        <div className="flex flex-col gap-4">
          {product.brand ? (
            <p className="tag text-muted-foreground">{product.brand}</p>
          ) : null}
          <h1 className="heading-1">{product.name}</h1>
          <div className="flex flex-wrap items-center gap-3">
            <span className="text-2xl font-semibold tabular-nums">
              {money(product.price_gross)}
            </span>
            <StockBadge
              stock={product.available_stock}
              labels={{
                inStock: t("product.inStock"),
                soldOut: t("product.soldOut"),
              }}
            />
          </div>
          {product.short_description ? (
            <p className="text-lg text-muted-foreground">
              {product.short_description}
            </p>
          ) : null}

          {product.variants.length > 1 ? (
            <section className="flex flex-col gap-2">
              <h2 className="heading-3">{t("product.variants")}</h2>
              <ul className="divide-y rounded-lg border">
                {product.variants.map((v) => (
                  <li
                    key={v.id}
                    className="flex items-center justify-between gap-4 px-4 py-3"
                  >
                    <span>
                      {v.options
                        .map((o) => `${o.type}: ${o.value}`)
                        .join(", ") || v.sku}
                    </span>
                    <span className="font-medium tabular-nums">
                      {money(v.price_gross)}
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          ) : null}

          <AddToCart
            product={product}
            soldOut={isSoldOut(product.available_stock)}
          />

          <div className="gold-rule" />
          <p className="whitespace-pre-line">
            {product.description ?? t("product.noDescription")}
          </p>
        </div>
      </article>
    </div>
  )
}
