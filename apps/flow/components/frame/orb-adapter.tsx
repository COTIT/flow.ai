"use client"

import { StrandsStatus } from "@genesy/voice/components/strands-status"

/**
 * The one place that draws the orb. Today: the shared StrandsStatus (reads the provider itself).
 * shortcut: swap for `<GenesyOrb {...useVoiceOrb()} size={size} label={…} />` once genesy-orb merges.
 */
export function OrbVisual({ size }: { size: number }) {
  return (
    <div
      style={{ width: size, height: size }}
      className="[&_.voice-orb]:size-full"
    >
      <StrandsStatus />
    </div>
  )
}
