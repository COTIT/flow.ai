export type Readiness = "available" | "off" | "not_ready" | "unknown"

/**
 * Office's `GET /api/voice/readiness` answer as one state. Switched off is 200
 * `{ enabled: false, ready: true }` (the container stays healthy while paused), or 503
 * `{ enabled: false, ready: false }` when the setting itself is invalid; switched on is 200 or 503
 * with `enabled: true` (apps/office/lib/voice/readiness.ts).
 */
export function readinessState(status: number, body: unknown): Readiness {
  const value = (body ?? {}) as { enabled?: unknown; ready?: unknown }
  if ((status === 200 || status === 503) && value.enabled === false)
    return "off"
  if (status === 200 && value.enabled === true && value.ready === true)
    return "available"
  if (status === 503 && value.enabled === true) return "not_ready"
  return "unknown"
}
