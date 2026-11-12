/**
 * The bento rule, one for every scene: a 12-column grid at ≥1024 px of results width, 6 at
 * 640–1023, 2 below. Size comes from the component type; the saved order stays the reading order.
 */
export type BentoSize = "small" | "wide" | "chart" | "full"

const SMALL = new Set(["metric_card", "comparison"])
const CHART = new Set(["line_chart", "bar_chart"])

export function bentoSize(type: string, count: number): BentoSize {
  if (count === 1) return "full"
  if (SMALL.has(type)) return "small"
  if (CHART.has(type)) return "chart"
  return "wide"
}

/** Container-query spans (the results area, so an open conversation rail counts). */
export const BENTO_CLASS: Record<BentoSize, string> = {
  small: "col-span-1 @min-[640px]:col-span-3 @min-[1024px]:col-span-3",
  wide: "col-span-2 @min-[640px]:col-span-6 @min-[1024px]:col-span-6 @min-[640px]:row-span-2",
  chart:
    "col-span-2 @min-[640px]:col-span-6 @min-[1024px]:col-span-6 @min-[640px]:row-span-2",
  full: "col-span-full",
}

/** Fixed row tracks from 640 px: a tile is 1 or 2 tracks, never stretched by a taller neighbour,
 * so the first screen shows whole tiles (at a glance). Content taller than its tile is clipped
 * with "Show all", which grows that tile only. On a phone tiles stack at their natural height. */
export const TRACK_PX = 132
export const BENTO_GRID =
  "grid grid-cols-2 gap-3 @min-[640px]:grid-cols-6 @min-[640px]:auto-rows-[132px] @min-[640px]:gap-4 @min-[1024px]:grid-cols-12"

/** A scene of one component: natural height, nothing to line up with. */
export const BENTO_SINGLE = "grid grid-cols-1 gap-3"
