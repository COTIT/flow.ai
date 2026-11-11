"use client"

import { useContext, useEffect, useSyncExternalStore } from "react"
import { useStore } from "zustand"

import { CartContext } from "@/components/shop-provider"

export function useCartStore() {
  const store = useContext(CartContext)
  if (!store) throw new Error("ShopProvider is missing")
  return store
}

/** The cart's lines and actions; `ready` is false until localStorage has been read. */
export function useCart() {
  const cartStore = useCartStore()
  useEffect(() => {
    if (!cartStore.persist.hasHydrated()) void cartStore.persist.rehydrate()
  }, [cartStore])
  const ready = useSyncExternalStore(
    (done) => cartStore.persist.onFinishHydration(done),
    () => cartStore.persist.hasHydrated(),
    () => false
  )
  return { ...useStore(cartStore), ready }
}
