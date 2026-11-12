export type OrbMode = "ambient" | "immersive" | "compact"
type Rect = { left: number; top: number; width: number; height: number }

/**
 * Scenes keep the orb compact; home shows it ambient, or immersive while a conversation opens.
 * Once the conversation has content it steps aside (compact) so the chat gets the full height.
 */
export function orbMode({
  pathname,
  resultOpen,
  immersive,
  conversing,
  sheetShown,
}: {
  pathname: string
  resultOpen?: boolean
  immersive?: boolean
  conversing?: boolean
  /** The phone's conversation sheet is on screen: it holds the orb (never covered by it). */
  sheetShown?: boolean
}): OrbMode {
  if (pathname !== "/" || resultOpen || sheetShown) return "compact"
  if (!immersive) return "ambient"
  return conversing ? "compact" : "immersive"
}

/** Translate + scale (origin top-left) that puts a `base`-px orb centred on `slot`, relative to `layer`. */
export function slotTransform(slot: Rect, layer: Rect, base: number) {
  const scale = Math.min(slot.width, slot.height) / base
  const size = base * scale
  return {
    x: slot.left - layer.left + (slot.width - size) / 2,
    y: slot.top - layer.top + (slot.height - size) / 2,
    scale,
  }
}

/** A critically damped spring sampled as a CSS `linear()` easing: settles without overshoot. */
export function springEasing(samples = 24, omega = 8) {
  const at = (t: number) => 1 - (1 + omega * t) * Math.exp(-omega * t)
  const end = at(1)
  const points = Array.from({ length: samples + 1 }, (_, i) =>
    i === 0
      ? 0
      : i === samples
        ? 1
        : Math.round((at(i / samples) / end) * 1000) / 1000
  )
  return `linear(${points.join(", ")})`
}
