import { useCallback, useEffect, useRef, useState } from 'react'

export type FarmLocationStatus =
  | 'idle'
  | 'unsupported'
  | 'acquiring'
  | 'acquired'
  | 'denied'
  | 'error'

export interface FarmLocationCoords {
  latitude: string
  longitude: string
}

/** `farm.latitude` / `longitude` are `numeric(9,6)` — match what the column stores. */
function round(value: number): string {
  return value.toFixed(6)
}

const PERMISSION_DENIED = 1

/**
 * One geolocation read, attempted once as soon as `enabled` turns true, and
 * again on demand — `docs/screens-and-components.md`'s `GpsCapture` states:
 * unsupported · permission denied · acquiring · acquired · manual entry
 * fallback (the last is the caller's job, not this hook's).
 *
 * `enabled` gates the automatic attempt only, not `retry()`: a caller whose
 * form already holds a value (an officer's own typing, or a restored draft)
 * passes `false` so mounting this hook never prompts for permission that
 * reading wouldn't need — see the register screen's `gpsEligible`.
 */
export function useFarmLocation(enabled: boolean) {
  const supported = typeof navigator !== 'undefined' && Boolean(navigator.geolocation)
  const [status, setStatus] = useState<FarmLocationStatus>(supported ? 'idle' : 'unsupported')
  const [coords, setCoords] = useState<FarmLocationCoords | null>(null)

  // Bumped on every attempt, so a reply from an attempt `retry()` has since
  // superseded is dropped rather than overwriting what came after it.
  const attempt = useRef(0)
  const autoStarted = useRef(false)

  const acquire = useCallback(() => {
    if (!supported) return
    const id = ++attempt.current
    setStatus('acquiring')
    navigator.geolocation.getCurrentPosition(
      (position) => {
        if (attempt.current !== id) return
        setCoords({
          latitude: round(position.coords.latitude),
          longitude: round(position.coords.longitude),
        })
        setStatus('acquired')
      },
      (error) => {
        if (attempt.current !== id) return
        setStatus(error.code === PERMISSION_DENIED ? 'denied' : 'error')
      },
      { timeout: 10_000 },
    )
  }, [supported])

  useEffect(() => {
    if (!enabled || autoStarted.current) return
    autoStarted.current = true
    acquire()
  }, [enabled, acquire])

  return { status, coords, retry: acquire }
}
