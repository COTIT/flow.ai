"use client"

import { usePathname, useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import { X } from "lucide-react"

import { Button } from "@genesy/ui/components/button"
import { useVoice } from "@genesy/voice/components/provider"
import { ReviewTitle } from "@genesy/voice/components/review-details"
import { clarifyOf, taskView, type TaskState } from "@genesy/voice/lib/task"
import { flowStore } from "@/lib/flow-store"
import { taskStore, useTask } from "@/lib/task-store"

const ENDED: TaskState[] = ["completed", "partial", "failed", "cancelled", "unknown"]

/** The task Genesy is on, from the provider's real state; follows every scene, never covers one. */
export function useTaskView() {
  const voice = useVoice()
  const decided = useTask((s) => s.decided)
  const result = useTask((s) => s.result)
  return taskView({
    ...voice,
    // A decided task ends with its own answer, never a later one.
    outcome: decided ? result : voice.outcome,
    decided,
    clarifying: Boolean(clarifyOf(voice.messages)),
  })
}

/** Opens the task scene (review, choice, receipt) on home, where the conversation lives. */
export function useOpenTask() {
  const router = useRouter()
  const pathname = usePathname()
  return () => {
    if (pathname === "/") flowStore.setState({ resultOpen: true })
    else router.push("/?view=task")
  }
}

/** A compact line while a task is active: state, an honest milestone, and only real controls. */
export function TaskCapsule() {
  const t = useTranslations("task")
  const voice = useVoice()
  const view = useTaskView()
  const decided = useTask((s) => s.decided)
  const open = useOpenTask()
  if (!view) return null
  const ended = ENDED.includes(view.state)
  const subject = voice.review ?? decided
  const batchRunning =
    view.state === "running" && voice.review?.kind === "batch"
  return (
    <div
      role="status"
      aria-label={t("label")}
      data-task-state={view.state}
      className="flex min-w-0 flex-wrap items-center gap-2 rounded-xl border border-(--gold) p-2"
    >
      <p className="mr-auto min-w-0 text-sm">
        <span className="text-xs font-semibold tracking-wide text-muted-foreground uppercase">
          {ended || view.state === "paused" ? t(`states.${view.state}`) : t("working")}
        </span>{" "}
        {subject ? (
          <span className="font-semibold break-words">
            <ReviewTitle review={subject} />
            {" · "}
          </span>
        ) : null}
        <span>{t(`milestones.${view.milestone}`)}</span>
        {view.counts ? (
          <span className="block text-xs text-muted-foreground">
            {t("counts", view.counts)}
          </span>
        ) : null}
      </p>
      {view.state === "needs_approval" || view.state === "waiting_input" ? (
        <Button className="min-h-11" onClick={open}>
          {t("review")}
        </Button>
      ) : null}
      {batchRunning ? (
        <Button
          variant="outline"
          className="min-h-11"
          onClick={() => void voice.stopBatch("pause")}
        >
          {t("pause")}
        </Button>
      ) : null}
      {ended || view.state === "paused" ? (
        <>
          <Button variant="outline" className="min-h-11" onClick={open}>
            {t("openReceipt")}
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="min-h-11 min-w-11"
            aria-label={t("dismiss")}
            onClick={() => taskStore.setState({ decided: null, result: null })}
          >
            <X aria-hidden />
          </Button>
        </>
      ) : null}
    </div>
  )
}
