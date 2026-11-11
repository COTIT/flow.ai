"use client"

import { useEffect, useState } from "react"
import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { Button } from "@genesy/ui/components/button"
import { useVoice } from "@genesy/voice/components/provider"
import { batchCounts } from "@genesy/voice/lib/task"
import { dockAsk } from "@/lib/pills"
import { outcomeKind, unseenOutcomes } from "@/lib/recovered-outcomes"
import { approvalOutcome, taskStore, useTask } from "@/lib/task-store"

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
  const tt = useTranslations("task")
  const voice = useVoice()
  const dismissed = useTask((s) => s.resumeDismissed)
  const workspace = useSession((s) => s.workspaceId)
  // Read once per mount: lines stay up while shown here, and are gone on the next visit.
  const [seen] = useState(() => readSeen(workspace))
  const decided = useTask((s) => s.decided)
  // The task card already shows its own approval's outcome (P6-3): never named twice.
  const outcomes = (idle && !dismissed ? unseenOutcomes(voice.recoveredTasks, seen) : []).filter(
    (o) => !decided || !approvalOutcome(decided, [o.task])
  )
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
      {outcomes.map(({ id: key, task: done }) => {
        // P8-6: a run found again (e.g. after logging in) says where it stands, in numbers.
        const run = done.data?.kind === "batch" ? done.data : undefined
        const counts = run?.steps?.length
          ? batchCounts({ id: key, state: run.state ?? "", nextIndex: 0, recoverable: false, steps: run.steps })
          : null
        return (
          <p key={key} data-recovered-outcome={outcomeKind(done.status)}>
            {t(outcomeKind(done.status), { summary: name(done.data) })}
            {run?.state === "running" || run?.state === "paused" ? ` ${tt(`states.${run.state}`)}.` : null}
            {counts ? ` ${tt("batch.result", counts)}` : null}
          </p>
        )
      })}
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
        {/* A status question to Genesy in a new conversation; nothing is resumed or sent again. */}
        {outcomes.some((o) => outcomeKind(o.task.status) === "unconfirmed") ? (
          <Button
            variant="outline"
            className="min-h-11"
            onClick={() => dockAsk.getState().ask?.(tt("receipt.checkMessage"))}
          >
            {tt("receipt.check")}
          </Button>
        ) : null}
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
