import { FLOW_CAPABILITIES } from "@genesy/voice/lib/capabilities"
import type { Uses } from "@genesy/voice/lib/rank"

/** Observed use only orders suggestions; it is kept on this device (decision Q6) for 90 days. */
const KEEP = 90 * 86_400_000
const IDS = new Set(FLOW_CAPABILITIES.map((c) => c.id))

type Activity = { uses: Uses; hintDismissed: boolean }

export const activityKey = (userId: string, workspaceId: string) =>
  `genesy.flow.activity:${userId}:${workspaceId}`

function load(key: string): Activity {
  try {
    const raw = JSON.parse(localStorage.getItem(key) ?? "null") as {
      uses?: Uses
      hintDismissed?: boolean
    } | null
    return { uses: raw?.uses ?? {}, hintDismissed: raw?.hintDismissed === true }
  } catch {
    return { uses: {}, hintDismissed: false }
  }
}

function save(key: string, activity: Activity) {
  try {
    localStorage.setItem(key, JSON.stringify({ v: 1, ...activity }))
  } catch {
    // Private mode or full storage: suggestions simply stay unpersonalised.
  }
}

export function readActivity(key: string, now = Date.now()): Activity {
  const { uses, hintDismissed } = load(key)
  const kept = Object.entries(uses).filter(
    ([id, use]) =>
      IDS.has(id) &&
      Number.isFinite(use?.n) &&
      Number.isFinite(use?.last) &&
      now - use.last <= KEEP
  )
  return { uses: Object.fromEntries(kept), hintDismissed }
}

export function recordUse(key: string, id: string, now = Date.now()) {
  if (!IDS.has(id)) return
  const activity = readActivity(key, now)
  activity.uses[id] = { n: (activity.uses[id]?.n ?? 0) + 1, last: now }
  save(key, activity)
}

export const dismissHint = (key: string) =>
  save(key, { ...readActivity(key), hintDismissed: true })

/** "Reset suggestions": forgets the counts; a dismissed hint stays dismissed. */
export const resetActivity = (key: string) =>
  save(key, { uses: {}, hintDismissed: readActivity(key).hintDismissed })
