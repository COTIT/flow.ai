import type { VoiceReview } from "@genesy/voice/lib/client-contract"

/**
 * The stock totals a review proposed, per warehouse, named from the review's own snapshot. What
 * the person approved: shown as such, never as a value the server confirmed.
 */
export function stockLevels(review: VoiceReview) {
  const changes = review.details.changes as { warehouses?: unknown } | undefined
  const slots =
    (review.details.current as
      | { locked?: { slots?: { warehouse_id?: unknown; warehouse_name?: unknown }[] } }
      | undefined)?.locked?.slots ?? []
  const levels = Array.isArray(changes?.warehouses)
    ? (changes.warehouses as { warehouse_id?: unknown; stock_lvl?: unknown }[])
    : []
  return levels.flatMap((entry) =>
    entry?.stock_lvl === undefined || entry.stock_lvl === null
      ? []
      : [
          {
            warehouse: String(entry.warehouse_id),
            name: String(slots.find((s) => s.warehouse_id === entry.warehouse_id)?.warehouse_name ?? ""),
            total: String(entry.stock_lvl),
          },
        ]
  )
}
