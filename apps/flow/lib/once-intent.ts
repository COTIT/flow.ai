import type { TranscriptRow } from "@genesy/voice/lib/helpers"

/** A lost answer never leaves a dead button: past this, the intent may be sent again. */
export const INTENT_LIMIT_MS = 20_000

/** One message a task-card button sent to Genesy, for the card's current subject (`key`). */
export type SentIntent = {
  key: string
  /** Last transcript row when it was sent; only rows after it can answer it. */
  after: string | undefined
  /** The error already showing when it was sent. */
  error: string
  at: number
}

export type IntentNow = {
  key: string
  rows: Pick<TranscriptRow, "id" | "role">[]
  error: string
  connected: boolean
}

/**
 * Still waiting for Genesy's answer to the intent: until a non-user row lands after the send, a
 * new error shows, the connection drops, the card's subject changes, or the limit passes.
 */
export function intentPending(intent: SentIntent | null, now: IntentNow) {
  if (!intent || intent.key !== now.key || !now.connected) return false
  if (now.error && now.error !== intent.error) return false
  if (Date.now() - intent.at >= INTENT_LIMIT_MS) return false
  const from = intent.after === undefined ? -1 : now.rows.findIndex((r) => r.id === intent.after)
  // Its row scrolled out of the capped transcript: many rows came since, so it was answered.
  if (intent.after !== undefined && from < 0) return false
  return !now.rows.slice(from + 1).some((r) => r.role !== "user")
}
