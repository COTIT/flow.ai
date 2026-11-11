"use client"

import { useTranslations } from "next-intl"

import { VoiceOrb } from "@genesy/voice/components/voice-orb"

/**
 * The one place that draws the orb: the Genesy orb wired to the session, in the app theme.
 * Its state words stay for assistive tech only; the caption under the slot shows them on screen.
 */
export function OrbVisual({ size }: { size: number }) {
  const t = useTranslations("shell.orb")
  return (
    <VoiceOrb
      size={size}
      label={(state) => t(state)}
      className="[&_.genesy-orb-label]:sr-only!"
    />
  )
}
