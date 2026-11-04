import type { StorefrontProduct } from "@genesy/api-client/model"
import { Badge } from "@genesy/ui/components/badge"

type Stock = Pick<StorefrontProduct, "stock_status" | "available_stock">

/**
 * The API's `stock_status`; without one, the old rule on `available_stock` (null when the
 * product isn't stock-tracked). Display only, up to a minute old: checkout's reply decides.
 */
const statusOf = ({ stock_status, available_stock }: Stock) =>
  stock_status ??
  (available_stock === null
    ? null
    : Number(available_stock) <= 0
      ? "sold_out"
      : "in_stock")

export const isSoldOut = (product: Stock) => statusOf(product) === "sold_out"

const tones = {
  in_stock: ["teal", "inStock"],
  low: ["gold", "low"],
  sold_out: ["slate", "soldOut"],
} as const

/** Mock badges: teal "In stock", gold "Few left", slate "Sold out"; nothing otherwise. */
export function StockBadge({
  product,
  labels,
  className,
}: {
  product: Stock
  labels: { inStock: string; low: string; soldOut: string }
  className?: string
}) {
  const tone = tones[statusOf(product) as keyof typeof tones]
  if (!tone) return null
  return (
    <Badge variant={tone[0]} className={className}>
      {labels[tone[1]]}
    </Badge>
  )
}
