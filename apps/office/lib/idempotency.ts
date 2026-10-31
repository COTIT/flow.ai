"use client"

import * as React from "react"

/**
 * One Idempotency-Key per attempt of an action that moves stock or money.
 * - `request`: pass as the generated hook's `request` option.
 * - `settle(error)`: call from `onSettled`. A reply from the API (success, 409, 422...) ends the
 *   attempt and makes a new key; a network failure keeps it, so the retry replays, not repeats.
 * - `renew()`: a new attempt, e.g. when its dialog opens.
 */
export function useIdempotencyKey() {
  const [key, setKey] = React.useState(() => crypto.randomUUID())
  const renew = React.useCallback(() => setKey(crypto.randomUUID()), [])
  return {
    request: { headers: { "Idempotency-Key": key } },
    settle: (error: unknown) => {
      if (!error || (error as { response?: unknown }).response) renew()
    },
    renew,
  }
}
