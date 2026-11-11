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

export type AgeLabel =
  { kind: "now" } | { kind: "minutes"; minutes: number } | { kind: "at" }

/** Coarse age of shown data: just now, whole minutes, then the clock time of the update. */
export function ageLabel(ageMs: number): AgeLabel {
  if (ageMs < 60_000) return { kind: "now" }
  if (ageMs < 3_600_000)
    return { kind: "minutes", minutes: Math.floor(ageMs / 60_000) }
  return { kind: "at" }
}

/** How long until the shown text can next change: twice a minute while young, once after 10 min. */
export const nextTickDelay = (ageMs: number) =>
  ageMs < 600_000 ? 30_000 : 60_000
