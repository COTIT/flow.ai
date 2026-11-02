import type { Metadata } from "next"
import Link from "next/link"
import { getTranslations } from "next-intl/server"
import { ArrowLeftIcon } from "lucide-react"

import { cn } from "@genesy/ui/lib/utils"

import { AddToCart } from "@/components/add-to-cart"
import { ProductGallery } from "@/components/product-gallery"
import { isSoldOut } from "@/components/stock-badge"
import { card, h2, tag } from "@/lib/look"
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

/**
 * Mock `ShopProduct` (bento). Its "This jar's passport" card and the property chips are left
 * out: the storefront product links no lot and has no such fields. So the photo tile spans
 * one row, not two.
 */
export default async function ProductPage({ params }: Props) {
  const { id } = await params
  const [t, product] = await Promise.all([getTranslations(), getProduct(id)])

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
        <div className={cn(card, "col-span-12 p-3 md:col-span-7 md:p-3")}>
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
          <AddToCart
            product={product}
            soldOut={isSoldOut(product.available_stock)}
          />
        </div>
        <div className={cn(card, "col-span-12")}>
          <div className="gold-rule w-12" />
          <h2 className={h2}>{t("product.about")}</h2>
          <p className="max-w-[760px] whitespace-pre-line text-muted-foreground">
            {product.description ?? t("product.noDescription")}
          </p>
        </div>
      </section>
    </>
  )
}
