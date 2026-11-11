import Link from "@/components/shop-link"

import { formatMoney } from "@genesy/ui/lib/money"
import { cn } from "@genesy/ui/lib/utils"

import { card, tag } from "@/lib/look"
import { shownPrice } from "@/lib/price"
import type { CategorisedProduct } from "@/lib/storefront"

import { Glare } from "./motion"
import { ProductImage } from "./product-image"
import { StockBadge } from "./stock-badge"

/** Product tile: photo 4:3 on top, then category (else brand), name, line, price and stock. */
export function ProductCard({
  product,
  locale,
  labels,
}: {
  product: CategorisedProduct
  locale: string
  labels: { inStock: string; low: string; soldOut: string; imageSoon: string }
}) {
  return (
    <Link
      href={`/products/${product.id}`}
      className={cn(
        card,
        "group w-full gap-0 overflow-hidden p-0 outline-none focus-visible:ring-3 focus-visible:ring-ring/50 md:gap-0 md:p-0"
      )}
    >
      <Glare radius="0">
        {/* The card's link names the product: the photo is decorative here. */}
        <ProductImage
          image={product.images?.[0]}
          alt=""
          label={labels.imageSoon}
          className="aspect-[4/3] w-full"
        />
      </Glare>
      <span className="flex flex-col gap-1.5 p-3 md:px-[18px] md:pt-4 md:pb-[18px]">
        {/* The category says more than a brand that is the shop's own name. */}
        {(product.category?.name ?? product.brand) ? (
          <span className={cn(tag, "text-muted-foreground")}>
            {product.category?.name ?? product.brand}
          </span>
        ) : null}
        <span className="text-base font-semibold group-hover:underline">
          {product.name}
        </span>
        {product.short_description ? (
          <span className="text-[13px] text-muted-foreground">
            {product.short_description}
          </span>
        ) : null}
        <span className="mt-1.5 flex flex-wrap items-center justify-between gap-3">
          <span className="text-base font-semibold text-gold-ink tabular-nums md:text-lg">
            {formatMoney(
              shownPrice(product),
              product.currency_decimals,
              product.currency,
              locale
            )}
          </span>
          <StockBadge product={product} labels={labels} />
        </span>
      </span>
    </Link>
  )
}
