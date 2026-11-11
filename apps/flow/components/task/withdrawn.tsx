"use client"

import { useTranslations } from "next-intl"

import { ReviewTitle } from "@genesy/voice/components/review-details"
import { reviewSummary } from "@genesy/voice/lib/review-changes"
import { useTask } from "@/lib/task-store"

/** The proposal a correction (or Cancel) replaced: shown once, as never sent. */
export function Withdrawn() {
  const t = useTranslations("task")
  const review = useTask((s) => s.withdrawn)
  if (!review) return null
  // Stock levels carry no field rows: the proposed totals are the request's own values, each
  // named by its warehouse from the review's snapshot.
  const levels =
    (review.details.changes as
      | { warehouses?: { warehouse_id?: unknown; stock_lvl?: unknown }[] }
      | undefined)?.warehouses ?? []
  const slots =
    (review.details.current as
      | { locked?: { slots?: { warehouse_id?: unknown; warehouse_name?: unknown }[] } }
      | undefined)?.locked?.slots ?? []
  const named = (id: unknown) =>
    slots.find((slot) => slot.warehouse_id === id)?.warehouse_name
  const rows = reviewSummary(review.details)?.rows ?? []
  const values = rows.length
    ? rows.map((row) => `${row.label ?? row.field} → ${row.after}`)
    : levels.map(
        (entry) => `${String(named(entry.warehouse_id) ?? "")} → ${String(entry.stock_lvl)}`
      )
  return (
    <p data-withdrawn className="rounded-xl border border-dashed p-2 text-sm text-muted-foreground">
      <span className="font-semibold">{t("withdrawn")}</span>
      {": "}
      <span className="line-through">
        <ReviewTitle review={review} />
        {values.map((value) => ` ${value}`)}
      </span>
    </p>
  )
}
