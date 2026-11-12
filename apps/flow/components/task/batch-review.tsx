"use client"

import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { useVoice } from "@genesy/voice/components/provider"
import { notIncludedRows } from "@genesy/voice/lib/presentation"
import { batchCounts } from "@genesy/voice/lib/task"
import { useOnceIntent } from "@/lib/use-once-intent"

/**
 * A batch in numbers: before approval, what matched and what is kept out (nothing is written for
 * it); once it ran, per-record outcomes. Pause stops further steps; done steps stay done.
 * A paused batch never resumes by itself: the rest needs a new review.
 */
export function BatchReview() {
  const t = useTranslations("task")
  const voice = useVoice()
  const batch = voice.batchProgress
  const review = voice.review?.kind === "batch" ? voice.review : null
  const counts = batch?.steps.length ? batchCounts(batch) : null
  const rest = useOnceIntent(`${batch?.id}:${counts?.notRun}`)
  if (!review && !batch) return null
  const ran = batch && !["proposed", "approved"].includes(batch.state)
  return (
    <section
      aria-label={t("label")}
      data-batch={batch?.state ?? "proposed"}
      className="flex min-w-0 flex-col gap-2 rounded-xl border p-3 text-sm"
    >
      {review ? (
        <p>
          {t("batch.matched", { count: review.steps?.length ?? 0 })}
          {" · "}
          {t("batch.exceptions", { count: notIncludedRows(review).length })}
        </p>
      ) : null}
      {ran && counts ? <p data-batch-result>{t("batch.result", counts)}</p> : null}
      {review && voice.busy ? (
        <Button
          variant="outline"
          className="min-h-11 self-start"
          onClick={() => void voice.stopBatch("pause")}
        >
          {t("pause")}
        </Button>
      ) : null}
      {batch?.state === "paused" && !review && counts?.notRun ? (
        <div className="flex flex-wrap items-center gap-2">
          <p className="mr-auto">{t("batch.continueText")}</p>
          <Button
            className="min-h-11"
            disabled={!voice.connected || voice.busy || rest.pending}
            aria-busy={rest.pending}
            onClick={() => void rest.send(t("batch.continueMessage", { count: counts.notRun }))}
          >
            {t("batch.continue")}
          </Button>
        </div>
      ) : null}
    </section>
  )
}
