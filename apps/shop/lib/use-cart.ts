"use client"

import { useEffect, useSyncExternalStore } from "react"
import { useStore } from "zustand"

import { cartStore } from "./cart"

const hydrated = {
  subscribe: (done: () => void) => cartStore.persist.onFinishHydration(done),
  get: () => cartStore.persist.hasHydrated(),
}

/** The cart's lines and actions; `ready` is false until localStorage has been read. */
export function useCart() {
  useEffect(() => {
    if (!cartStore.persist.hasHydrated()) void cartStore.persist.rehydrate()
  }, [])
  const ready = useSyncExternalStore(
    hydrated.subscribe,
    hydrated.get,
    () => false
  )
  return { ...useStore(cartStore), ready }
}
