import { cn } from "@genesy/ui/lib/utils"

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
  const soldOut = Number(stock) <= 0
  return (
    <span
      className={cn(
        "inline-flex w-fit rounded-sm px-2 py-1 tag",
        soldOut
          ? "bg-muted text-muted-foreground"
          : "bg-teal text-teal-foreground",
        className
      )}
    >
      {soldOut ? labels.soldOut : labels.inStock}
    </span>
  )
}
