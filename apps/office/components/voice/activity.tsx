"use client"

import { CircleCheck, CircleAlert } from "lucide-react"
import { useTranslations } from "next-intl"
import ThoughtLine from "@/components/rb/ThoughtLine"
import { useVoice } from "./provider"
import { activityState } from "./presentation"

const PENDING = new Set([
  "connecting",
  "upgrading",
  "thinking",
  "executing",
  "uploading",
  "checking",
])
const ALERT = new Set(["failed", "unknown", "partial"])

/** Status line: a breathing thought line with the task's steps while the agent works. */
export function VoiceActivity({ compact = false }: { compact?: boolean }) {
  const voice = useVoice()
  const t = useTranslations("voice")
  const state = activityState({
    ...voice,
    outcome: voice.outcome?.status,
    review: Boolean(voice.review),
    uploading: voice.attachments.some((file) => file.state === "uploading"),
  })
  const pending = PENDING.has(state)
  const text = t(`activity.${state}`)
  if (compact)
    return (
      <p
        role="status"
        aria-live="polite"
        aria-busy={pending}
        data-state={state}
        className="flex min-h-6 items-center justify-center gap-2 text-sm font-medium"
      >
        {text}
      </p>
    )
  return (
    <div
      role="status"
      aria-live="polite"
      aria-busy={pending}
      data-voice-activity=""
      data-state={state}
      className="flex min-h-6 w-full max-w-md flex-col items-center justify-center text-sm text-muted-foreground"
    >
      <ThoughtLine
        label={text}
        doneLabel={text}
        working={pending}
        steps={pending ? voice.steps : []}
        showTimer={pending}
        collapseOnSettle
        glyph={
          pending ? (
            "sparkle"
          ) : state === "completed" ? (
            <CircleCheck aria-hidden className="size-4" />
          ) : ALERT.has(state) ? (
            <CircleAlert aria-hidden className="size-4" />
          ) : (
            "dot"
          )
        }
        color="var(--muted-foreground)"
        glyphColor="var(--gold)"
        fontSize={14}
      />
    </div>
  )
}
