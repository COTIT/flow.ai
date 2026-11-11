"use client"

import { createContext, useState } from "react"
import { createCartStore } from "@/lib/cart"
import { ShopContext } from "@/lib/shop-context"

export const CartContext = createContext<ReturnType<
  typeof createCartStore
> | null>(null)

export function ShopProvider({
  base,
  name,
  children,
}: {
  base: string
  name: string
  children: React.ReactNode
}) {
  const [cart] = useState(() => createCartStore(base))
  return (
    <ShopContext value={{ base, name }}>
      <CartContext value={cart}>{children}</CartContext>
    </ShopContext>
  )
}
