"use client"

import { useEffect, useState } from "react"
import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { Button } from "@genesy/ui/components/button"
import { useVoice } from "@genesy/voice/components/provider"
import { outcomeKind, unseenOutcomes } from "@/lib/recovered-outcomes"
import { taskStore, useTask } from "@/lib/task-store"

const seenKey = (workspace: string) => `flow-seen-outcomes:${workspace}`
function readSeen(workspace: string | null): string[] {
  if (!workspace || typeof window === "undefined") return []
  try {
    const value: unknown = JSON.parse(localStorage.getItem(seenKey(workspace)) ?? "[]")
    return Array.isArray(value) ? value.filter((id) => typeof id === "string") : []
  } catch {
    return []
  }
}

/**
 * With no live conversation (ended, or after a reload / in another tab / back in this workspace):
 * the voice session is gone, the work is not. A review still within its recovery window reopens
 * visually (no voice needed); what ran, failed or is unknown since this workspace last saw it is
 * named once. A dropped request is VoiceUnfinished's (new conversation). Only the provider's
 * recovered tasks for this workspace are used; nothing is confirmed or sent again from here.
 */
export function ResumePrompt({ idle }: { idle: boolean }) {
  const t = useTranslations("task.resume")
  const voice = useVoice()
  const dismissed = useTask((s) => s.resumeDismissed)
  const workspace = useSession((s) => s.workspaceId)
  // Read once per mount: lines stay up while shown here, and are gone on the next visit.
  const [seen] = useState(() => readSeen(workspace))
  const outcomes = idle && !dismissed ? unseenOutcomes(voice.recoveredTasks, seen) : []
  const shown = outcomes.map((o) => o.id).join(",")
  useEffect(() => {
    if (!workspace || !shown) return
    try {
      const ids = [...new Set([...readSeen(workspace), ...shown.split(",")])].slice(-200)
      localStorage.setItem(seenKey(workspace), JSON.stringify(ids))
    } catch {
      // Storage refused (private mode): the line may show again next time; nothing else changes.
    }
  }, [workspace, shown])
  const task = voice.recoveredTasks.find(
    (item) => item.status === "awaiting_review" && item.data?.recoverable
  )
  if (!idle || dismissed || voice.review || (!task && !outcomes.length)) return null
  const id = task?.data?.batchId ?? task?.data?.actionId
  const name = (data?: { summary?: string; operation?: string }) =>
    data?.summary ?? data?.operation ?? ""
  return (
    <section
      aria-labelledby="flow-resume-title"
      className="flex min-w-0 flex-col gap-2 rounded-xl border border-(--gold) p-3 text-sm"
    >
      <h2 id="flow-resume-title" className="text-xs font-semibold uppercase">
        {task ? t("title") : t("outcomes")}
      </h2>
      {outcomes.map(({ id: key, task: done }) => (
        <p key={key} data-recovered-outcome={outcomeKind(done.status)}>
          {t(outcomeKind(done.status), { summary: name(done.data) })}
        </p>
      ))}
      {task ? <p>{t("awaiting", { summary: name(task.data) })}</p> : null}
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
        {id && task ? (
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
