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

/** One tile as the band planner sees it: its type, and whether it has nothing to show. */
export type PlanInput = { id: string; type: string; empty?: boolean }
/** Where it goes: its row ("band"), its column span, and whether it is a slim empty line. */
export type PlanTile = {
  id: string
  band: number
  span: number
  slim: boolean
  /** Grid rows it spans: 2 for a big tile beside two stacked metrics. */
  rows: number
  /** A metric stacked with another in one column beside a big tile. */
  stack: boolean
}

/**
 * The fit rule (bento fit): tiles are laid out in rows ("bands") whose spans always add up to the
 * full width, in the saved order (so reading and Tab order never change). Metrics (and empty
 * components, drawn as slim lines) pair up; at 12 columns up to two metrics share a row with the
 * next big tile, which takes the rest; a row with nothing to share widens its tiles to the width.
 */
export function planBands(
  list: readonly PlanInput[],
  cols: 12 | 6 | 2
): PlanTile[] {
  const small = (t: PlanInput) => !!t.empty || SMALL.has(t.type)
  const out: PlanTile[] = []
  let band = 0
  const row = (
    tiles: readonly PlanInput[],
    spans: readonly number[],
    stacked = false
  ) => {
    tiles.forEach((t, i) =>
      out.push({
        id: t.id,
        band,
        span: spans[i]!,
        slim: !!t.empty,
        // Two metrics stack in one column; the big tile beside them spans both rows.
        rows: stacked && i === tiles.length - 1 ? tiles.length - 1 : 1,
        stack: stacked && i < tiles.length - 1,
      })
    )
    band++
  }
  /** Spans that share `width` evenly; the last tile takes what is left. */
  const even = (n: number, width: number) =>
    Array.from({ length: n }, (_, i) =>
      i < n - 1
        ? Math.floor(width / n)
        : width - Math.floor(width / n) * (n - 1)
    )
  const unit = cols === 12 ? 3 : cols === 6 ? 3 : 1 // a metric's width
  const perRow = cols / unit // metrics a row holds
  let i = 0
  while (i < list.length) {
    let k = 0
    while (i + k < list.length && k < perRow && small(list[i + k]!)) k++
    if (k > 0) {
      const next = list[i + k]
      // At 12 columns a big tile joins one or two metrics and takes the rest of the row.
      if (cols === 12 && next && !small(next) && k * unit <= cols / 2) {
        // One metric sits beside the big tile; two stack in that one column (no empty height).
        row(
          [...list.slice(i, i + k), next],
          [...Array(k).fill(unit), cols - unit],
          k === 2
        )
        i += k + 1
      } else {
        row(list.slice(i, i + k), even(k, cols))
        i += k
      }
      continue
    }
    const next = list[i + 1]
    if (cols === 12 && next && !small(next)) {
      row([list[i]!, next], [6, 6])
      i += 2
    } else {
      row([list[i]!], [cols])
      i += 1
    }
  }
  return out
}
