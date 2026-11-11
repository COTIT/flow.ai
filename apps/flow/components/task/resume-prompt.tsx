"use client"

import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { useVoice } from "@genesy/voice/components/provider"
import { taskStore, useTask } from "@/lib/task-store"

/**
 * After a conversation ended: the voice session is gone, the work is not. A review still within
 * its recovery window reopens visually (no voice needed). A dropped request is VoiceUnfinished's
 * (new conversation). Only the existing recovery pointer is used; nothing claims the old session lives.
 */
export function ResumePrompt({ ended }: { ended: boolean }) {
  const t = useTranslations("task.resume")
  const voice = useVoice()
  const dismissed = useTask((s) => s.resumeDismissed)
  const task = voice.recoveredTasks.find(
    (item) => item.status === "awaiting_review" && item.data?.recoverable
  )
  if (!ended || dismissed || voice.review || !task) return null
  const id = task?.data?.batchId ?? task?.data?.actionId
  return (
    <section
      aria-labelledby="flow-resume-title"
      className="flex min-w-0 flex-col gap-2 rounded-xl border border-(--gold) p-3 text-sm"
    >
      <h2 id="flow-resume-title" className="text-xs font-semibold uppercase">
        {t("title")}
      </h2>
      <p>{t("awaiting", { summary: task.data?.summary ?? task.data?.operation ?? "" })}</p>
      <div className="flex flex-wrap justify-end gap-2">
        <Button
          variant="ghost"
          className="min-h-11"
          onClick={() => {
            taskStore.setState({ resumeDismissed: true })
            void voice.closeRecovery()
          }}
        >
          {t("dismiss")}
        </Button>
        {id ? (
          <Button
            className="min-h-11"
            disabled={voice.busy}
            onClick={() =>
              void voice.restore(id, task.data?.kind === "batch" ? "batch" : "action")
            }
          >
            {t("continue")}
          </Button>
        ) : null}
      </div>
    </section>
  )
}
