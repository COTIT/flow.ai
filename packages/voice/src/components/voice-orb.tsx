"use client"

import { useVoice } from "./provider"
import { GenesyOrb, type GenesyOrbProps } from "./genesy-orb"
import { orbState, type OrbState } from "../lib/orb"

/** The Genesy orb wired to the voice session: state from the session, level from the right voice. */
export function VoiceOrb({
  label,
  ...props
}: Omit<GenesyOrbProps, "state" | "level" | "label"> & {
  label: (state: OrbState) => string
}) {
  const voice = useVoice()
  const state = orbState(voice)
  // Listening follows the user's microphone, speaking follows Genesy's voice.
  const level =
    state === "speaking"
      ? voice.outputLevel
      : state === "listening"
        ? voice.inputLevel
        : undefined
  return (
    <GenesyOrb {...props} state={state} level={level} label={label(state)} />
  )
}
