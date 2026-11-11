export type Freshness = {
  kind: "loading" | "fresh" | "refreshing" | "stale" | "failed"
  seconds: number
}

/** What the "Updated 32 s ago" line says about one query's answer. */
export function freshness({
  updatedAt,
  fetching,
  failed,
  now,
  staleAfter,
}: {
  updatedAt: number
  fetching: boolean
  failed: boolean
  now: number
  staleAfter: number
}): Freshness {
  const seconds = Math.max(0, Math.floor((now - updatedAt) / 1000))
  if (!updatedAt) return { kind: "loading", seconds: 0 }
  if (fetching) return { kind: "refreshing", seconds }
  if (failed) return { kind: "failed", seconds }
  // Two missed refreshes: say the numbers may be outdated.
  if (now - updatedAt > 2 * staleAfter) return { kind: "stale", seconds }
  return { kind: "fresh", seconds }
}
