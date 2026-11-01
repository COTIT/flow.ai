import { createStore } from "zustand/vanilla"
import { useStore } from "zustand"

import type { LineRequest } from "@genesy/api-client/model"

/** One cart line, with the catalog figures it was rung up at (display + `unit_price`). */
export type CartLine = {
  productId: string
  variantId: string | null
  name: string
  /** Catalog price excl VAT, minor units: sent as `unit_price`. */
  unitNet: number
  /** One unit incl VAT from the catalog: the offline estimate only. */
  unitGross: number | null
  /** shortcut: whole pieces only; weighed goods (2-decimal quantities) need the quote API first. */
  quantity: number
}

type Cart = {
  lines: CartLine[]
  /** The sale's `client_ref`, made on the first charge and kept for every retry of it. */
  clientRef: string | null
  add: (line: Omit<CartLine, "quantity">) => void
  setQuantity: (index: number, quantity: number) => void
  remove: (index: number) => void
  startCharge: () => string
  clear: () => void
}

/** The open sale; kept in memory so walking to the drawer screen doesn't lose it. */
export const cartStore = createStore<Cart>()((set, get) => ({
  lines: [],
  clientRef: null,
  add: (line) => {
    const lines = get().lines
    const i = lines.findIndex(
      (l) => l.productId === line.productId && l.variantId === line.variantId
    )
    set({
      clientRef: null,
      lines:
        i < 0
          ? [...lines, { ...line, quantity: 1 }]
          : lines.map((l, j) =>
              j === i ? { ...l, quantity: l.quantity + 1 } : l
            ),
    })
  },
  setQuantity: (index, quantity) =>
    set({
      clientRef: null,
      lines: get().lines.map((l, j) => (j === index ? { ...l, quantity } : l)),
    }),
  remove: (index) =>
    set({ clientRef: null, lines: get().lines.filter((_, j) => j !== index) }),
  startCharge: () => {
    const ref = get().clientRef ?? crypto.randomUUID()
    set({ clientRef: ref })
    return ref
  },
  clear: () => set({ lines: [], clientRef: null }),
}))

export function useCart<T>(pick: (s: Cart) => T): T {
  return useStore(cartStore, pick)
}

/** The API's sale lines: the till's catalog net price as `unit_price`. */
export function toSaleLines(lines: CartLine[]): LineRequest[] {
  return lines.map((l) => ({
    product_id: l.productId,
    ...(l.variantId ? { product_variant_id: l.variantId } : {}),
    quantity: String(l.quantity),
    unit_price: l.unitNet,
  }))
}

/**
 * Offline only, labelled "≈": catalog gross × pieces. Online the cart shows the API's quote.
 * null when a line has no gross price.
 */
export function estimate(lines: CartLine[]): number | null {
  return lines.some((l) => l.unitGross == null)
    ? null
    : lines.reduce((sum, l) => sum + l.unitGross! * l.quantity, 0)
}
