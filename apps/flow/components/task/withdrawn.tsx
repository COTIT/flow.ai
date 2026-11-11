"use client"

import { useTranslations } from "next-intl"

import { ReviewTitle } from "@genesy/voice/components/review-details"
import { reviewSummary } from "@genesy/voice/lib/review-changes"
import { stockLevels } from "@/lib/proposed"
import { useTask } from "@/lib/task-store"

/** The proposal a correction (or Cancel) replaced: shown once, as never sent. */
export function Withdrawn() {
  const t = useTranslations("task")
  const review = useTask((s) => s.withdrawn)
  if (!review) return null
  // Stock levels carry no field rows: the proposed totals, each named by its warehouse.
  const rows = reviewSummary(review.details)?.rows ?? []
  const values = rows.length
    ? rows.map((row) => `${row.label ?? row.field} → ${row.after}`)
    : stockLevels(review).map((level) => `${level.name} → ${level.total}`)
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
