import { useState } from "react"

/**
 * Lets one async action run at a time, decided synchronously: `disabled={isPending}` only applies
 * after React re-renders, so two clicks in the same frame would both start. Returns undefined
 * when an action is already running.
 */
export function createSyncGuard() {
  let busy = false
  return <T>(start: () => Promise<T>): Promise<T> | undefined => {
    if (busy) return undefined
    busy = true
    try {
      return start().finally(() => (busy = false))
    } catch (e) {
      busy = false
      throw e
    }
  }
}

/** One guard per component instance, stable across renders. */
export const useSyncGuard = () => useState(createSyncGuard)[0]
