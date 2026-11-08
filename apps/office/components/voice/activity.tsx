"use client"

import { CircleCheck, CircleAlert } from "lucide-react"
import { useTranslations } from "next-intl"
import ThoughtLine from "@/components/rb/ThoughtLine"
import { useVoice } from "./provider"
import { activityPending, activityState } from "./presentation"

const ALERT = new Set(["failed", "unknown", "partial"])

function useActivity() {
  const voice = useVoice()
  const state = activityState({
    ...voice,
    outcome: voice.outcome?.status,
    review: Boolean(voice.review),
    uploading: voice.attachments.some((file) => file.state === "uploading"),
  })
  return { voice, state, pending: activityPending(state) }
}

/** Short status under the avatar; while the agent works, the thinking line in the chat says it. */
export function VoiceActivity({ compact = false }: { compact?: boolean }) {
  const { state, pending } = useActivity()
  const t = useTranslations("voice")
  return (
    <p
      role="status"
      aria-live="polite"
      aria-busy={pending}
      data-voice-activity={compact ? undefined : ""}
      data-state={state}
      className={
        pending && !compact
          ? "sr-only"
          : `flex min-h-6 items-center justify-center gap-2 text-sm ${compact ? "font-medium" : "text-muted-foreground"}`
      }
    >
      {!pending && state === "completed" ? (
        <CircleCheck aria-hidden className="size-4 shrink-0" />
      ) : !pending && ALERT.has(state) ? (
        <CircleAlert aria-hidden className="size-4 shrink-0" />
      ) : null}
      {t(`activity.${state}`)}
    </p>
  )
}

/** Inline thinking row at the end of the chat: label, live timer and the task's steps. */
export function VoiceThinking() {
  const { voice, state, pending } = useActivity()
  const t = useTranslations("voice")
  if (!pending) return null
  return (
    <div
      data-voice-thinking=""
      aria-hidden="true"
      // ThoughtLine's expand toggle is a button; keep it out of the tab order too.
      inert
      className="max-w-[80%] self-start rounded-2xl rounded-bl-md border border-(--hair) bg-card px-4 py-2 text-sm"
    >
      <ThoughtLine
        label={t(`activity.${state}`)}
        working
        steps={voice.steps}
        showTimer
        collapseOnSettle
        glyph="sparkle"
        color="var(--foreground)"
        glyphColor="var(--gold)"
        fontSize={14}
      />
    </div>
  )
}
