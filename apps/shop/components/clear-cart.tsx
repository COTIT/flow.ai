"use client"

import { useEffect } from "react"

import { useCartStore } from "@/lib/use-cart"

/** Empties the cart once its order is paid. */
export function ClearCart() {
  const cartStore = useCartStore()
  useEffect(() => {
    void Promise.resolve(cartStore.persist.rehydrate()).then(() =>
      cartStore.getState().clear()
    )
  }, [cartStore])
  return null
}
