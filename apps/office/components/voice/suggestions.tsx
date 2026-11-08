"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { useRights } from "@/lib/can"
import {
  allowedFor,
  followUpFor,
  moduleFor,
  pickSuggestions,
  rotated,
  type Available,
} from "@/lib/voice/capabilities"
import { useVoice } from "./provider"

const DAY = 86_400_000

/** Start chips, the one follow-up chip and the text box hint, from the curated capability list. */
export function useVoiceTips(capabilities: Available[], from?: string) {
  const t = useTranslations("voice")
  const voice = useVoice()
  const rights = useRights()
  const allowed = rights.read ? allowedFor(capabilities, rights.may) : []
  const [day] = useState(() => Math.floor(Date.now() / DAY))
  // Chips fill a text box: shown before starting, and in text sessions (voice mode has no box).
  const textBox = voice.connected ? voice.textOnly : !voice.starting
  const last = voice.messages.at(-1)
  const next = followUpFor(
    allowed,
    voice.outcome?.status === "completed"
      ? voice.outcome.operationId
      : undefined
  )
  const chips = !textBox
    ? []
    : !voice.messages.length && !voice.outcome
      ? pickSuggestions(allowed, moduleFor(from), day)
      : next && voice.connected && !voice.busy && last?.role !== "user"
        ? [next]
        : []
  // A new hint after each sent message, without a timer.
  const sent = voice.messages.filter((row) => row.role === "user").length
  const tips = rotated(allowed, day)
  const tip = tips[(day + sent) % (tips.length || 1)]
  return {
    chips,
    placeholder: tip
      ? t("tryPrompt", { prompt: t(`capabilities.${tip.id}.prompt`) })
      : undefined,
  }
}

export function VoiceSuggestions({
  items,
  onPick,
}: {
  items: Available[]
  onPick: (prompt: string) => void
}) {
  const t = useTranslations("voice")
  if (!items.length) return null
  return (
    <div
      role="group"
      aria-label={t("suggestions")}
      className="flex flex-wrap justify-center gap-2"
    >
      {items.map((item, i) => (
        <button
          key={item.id}
          type="button"
          onClick={() => onPick(t(`capabilities.${item.id}.prompt`))}
          className={`min-h-11 rounded-full border border-(--hair) bg-card/60 px-4 py-2 text-sm text-foreground transition-transform duration-100 hover:border-(--gold) focus-visible:ring-2 focus-visible:ring-(--gold) focus-visible:outline-none active:scale-[0.97] motion-reduce:transition-none${i > 2 ? "max-sm:hidden" : ""}`}
        >
          {t(`capabilities.${item.id}.label`)}
        </button>
      ))}
    </div>
  )
}
