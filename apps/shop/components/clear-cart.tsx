"use client"

import { useEffect } from "react"

import { cartStore } from "@/lib/cart"

/** Empties the cart once its order is paid. */
export function ClearCart() {
  useEffect(() => {
    void Promise.resolve(cartStore.persist.rehydrate()).then(() =>
      cartStore.getState().clear()
    )
  }, [])
  return null
}
