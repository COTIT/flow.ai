import { Badge } from "@genesy/ui/components/badge"

/** Display only, from the API's (up to a minute old) number; checkout's reply decides. */
export const isSoldOut = (stock: string | null) =>
  stock !== null && Number(stock) <= 0

/** Availability from the API's `available_stock` string; nothing when not stock-tracked. */
export function StockBadge({
  stock,
  labels,
  className,
}: {
  stock: string | null
  labels: { inStock: string; soldOut: string }
  className?: string
}) {
  if (stock === null) return null
  const soldOut = isSoldOut(stock)
  return (
    <Badge variant={soldOut ? "slate" : "teal"} className={className}>
      {soldOut ? labels.soldOut : labels.inStock}
    </Badge>
  )
}
