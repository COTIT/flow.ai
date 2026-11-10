export type Readiness = "available" | "off" | "not_ready" | "unknown"

/** Office's `GET /api/voice/readiness` answer (200 or 503 `{ enabled, ready }`) as one state. */
export function readinessState(status: number, body: unknown): Readiness {
  const value = (body ?? {}) as { enabled?: unknown; ready?: unknown }
  if (status === 200 && value.ready === true) return "available"
  if (status === 503 && value.enabled === false) return "off"
  if (status === 503 && value.enabled === true) return "not_ready"
  return "unknown"
}
