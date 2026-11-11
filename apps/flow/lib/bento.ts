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
  wide: "col-span-2 @min-[640px]:col-span-6 @min-[1024px]:col-span-6",
  chart:
    "col-span-2 @min-[640px]:col-span-6 @min-[1024px]:col-span-6 @min-[640px]:row-span-2",
  full: "col-span-full",
}

/** The grid itself; rows grow with content (labels never clip), charts take two. */
export const BENTO_GRID =
  "grid grid-cols-2 gap-3 @min-[640px]:grid-cols-6 @min-[640px]:auto-rows-[minmax(132px,auto)] @min-[640px]:gap-4 @min-[1024px]:grid-cols-12"
