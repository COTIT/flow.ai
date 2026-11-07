"use client"

import { CircleCheck, CircleAlert, LoaderCircle } from "lucide-react"
import { useTranslations } from "next-intl"
import { useVoice } from "./provider"
import { activityState } from "./presentation"

export function VoiceActivity({ compact = false }: { compact?: boolean }) {
  const voice = useVoice()
  const t = useTranslations("voice")
  const state = activityState({
    ...voice,
    outcome: voice.outcome?.status,
    review: Boolean(voice.review),
    uploading: voice.attachments.some((file) => file.state === "uploading"),
  })
  const pending = [
    "connecting",
    "upgrading",
    "thinking",
    "executing",
    "uploading",
    "checking",
  ].includes(state)
  return (
    <p
      role="status"
      aria-live="polite"
      aria-busy={pending}
      data-voice-activity={compact ? undefined : ""}
      data-state={state}
      className={`flex min-h-6 items-center justify-center gap-2 text-sm ${compact ? "font-medium" : "text-muted-foreground"}`}
    >
      {pending ? (
        <LoaderCircle
          aria-hidden
          className="size-4 shrink-0 animate-spin motion-reduce:animate-none"
        />
      ) : state === "completed" ? (
        <CircleCheck aria-hidden className="size-4 shrink-0" />
      ) : ["failed", "unknown", "partial"].includes(state) ? (
        <CircleAlert aria-hidden className="size-4 shrink-0" />
      ) : null}
      {t(`activity.${state}`)}
    </p>
  )
}
