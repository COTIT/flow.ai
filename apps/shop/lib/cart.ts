import { createJSONStorage, persist } from "zustand/middleware"
import { createStore } from "zustand/vanilla"

/**
 * One cart line. Name and price are a snapshot from the product page, for display only:
 * the order the API creates at checkout sets the real prices, totals and stock.
 */
export type CartLine = {
  productId: string
  variantId: string | null
  quantity: number
  name: string
  variantLabel: string | null
  priceGross: number
  currency: string
  decimals: number
}

// The storefront checkout takes 1 to 50 lines of at most 1000 each.
const MAX_LINES = 50
const MAX_QUANTITY = 1000

const same = (l: CartLine, productId: string, variantId: string | null) =>
  l.productId === productId && l.variantId === variantId

const clamp = (q: number) => Math.min(MAX_QUANTITY, Math.trunc(q))

export function addLine(lines: CartLine[], line: CartLine): CartLine[] {
  if (lines.some((l) => same(l, line.productId, line.variantId)))
    return lines.map((l) =>
      same(l, line.productId, line.variantId)
        ? { ...l, quantity: clamp(l.quantity + line.quantity) }
        : l
    )
  return lines.length >= MAX_LINES
    ? lines
    : [...lines, { ...line, quantity: clamp(line.quantity) }]
}

/** Below 1 removes the line. */
export function setQuantity(
  lines: CartLine[],
  productId: string,
  variantId: string | null,
  quantity: number
): CartLine[] {
  const q = clamp(quantity)
  return q < 1
    ? lines.filter((l) => !same(l, productId, variantId))
    : lines.map((l) =>
        same(l, productId, variantId) ? { ...l, quantity: q } : l
      )
}

export const cartCount = (lines: CartLine[]) =>
  lines.reduce((n, l) => n + l.quantity, 0)

type Cart = {
  lines: CartLine[]
  add: (line: CartLine) => void
  setQuantity: (
    productId: string,
    variantId: string | null,
    quantity: number
  ) => void
  clear: () => void
}

/** The browser's cart, kept in localStorage (`genesy.cart`). Rehydrated after first render. */
export const cartStore = createStore<Cart>()(
  persist(
    (set) => ({
      lines: [],
      add: (line) => set((s) => ({ lines: addLine(s.lines, line) })),
      setQuantity: (productId, variantId, quantity) =>
        set((s) => ({
          lines: setQuantity(s.lines, productId, variantId, quantity),
        })),
      clear: () => set({ lines: [] }),
    }),
    {
      name: "genesy.cart",
      storage: createJSONStorage(() => localStorage),
      partialize: ({ lines }) => ({ lines }),
      // Server HTML has an empty cart; read storage after the first render so they match.
      skipHydration: true,
    }
  )
)
