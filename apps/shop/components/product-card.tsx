import Link from "next/link"

import type { StorefrontProduct } from "@genesy/api-client/model"
import { formatMoney } from "@genesy/ui/lib/money"

import { ProductImage } from "./product-image"
import { StockBadge } from "./stock-badge"

export function ProductCard({
  product,
  locale,
  labels,
}: {
  product: StorefrontProduct
  locale: string
  labels: { inStock: string; soldOut: string; imageSoon: string }
}) {
  return (
    <Link
      href={`/products/${product.id}`}
      className="group flex w-full flex-col overflow-hidden rounded-lg border bg-card text-card-foreground transition-colors outline-none hover:border-gold focus-visible:ring-3 focus-visible:ring-ring/50"
    >
      <ProductImage label={labels.imageSoon} />
      <div className="flex flex-1 flex-col gap-2 p-4">
        {product.brand ? (
          <p className="tag text-muted-foreground">{product.brand}</p>
        ) : null}
        <h2 className="font-medium group-hover:underline">{product.name}</h2>
        {product.short_description ? (
          <p className="line-clamp-2 text-sm text-muted-foreground">
            {product.short_description}
          </p>
        ) : null}
        <div className="mt-auto flex items-center justify-between gap-2 pt-2">
          <span className="font-semibold tabular-nums">
            {formatMoney(
              product.price_gross,
              product.currency_decimals,
              product.currency,
              locale
            )}
          </span>
          <StockBadge stock={product.available_stock} labels={labels} />
        </div>
      </div>
    </Link>
  )
}
