"use client"

import { useTranslations } from "next-intl"

import { VoiceActivity } from "@genesy/voice/components/activity"
import { useVoice } from "@genesy/voice/components/provider"
import { sessionPhase } from "@genesy/voice/lib/presentation"
import { useFlow } from "@/lib/flow-store"

/** Short state words under the orb: never colour or motion alone. */
export function OrbCaption() {
  const t = useTranslations()
  const voice = useVoice()
  const endedByUser = useFlow((s) => s.endedByUser)
  const { phase } = sessionPhase({ ...voice, endedByUser })
  if (phase === "live") return <VoiceActivity compact />
  return (
    <p className="text-sm text-muted-foreground" data-caption={phase}>
      {phase === "ended" ? t("shell.ended") : t("voice.welcome")}
    </p>
  )
}
