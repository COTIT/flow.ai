import { describe, expect, it, vi } from "vitest"

import {
  addLine,
  cartCount,
  createCartStore,
  setQuantity,
  type CartLine,
} from "./cart"

const honey = (
  quantity: number,
  variantId: string | null = "v1"
): CartLine => ({
  productId: "p1",
  variantId,
  quantity,
  name: "Honey",
  variantLabel: "250 g",
  priceGross: 1499,
  currency: "USD",
  decimals: 2,
})

describe("cart lines", () => {
  it("isolates workspace carts, including payment completion clearing", () => {
    const saved = new Map<string, string>()
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => saved.get(key) ?? null,
      setItem: (key: string, value: string) => saved.set(key, value),
      removeItem: (key: string) => saved.delete(key),
    })
    const first = createCartStore("/s/first")
    const second = createCartStore("/s/second")
    const legacy = createCartStore()
    first.getState().add(honey(2))
    second.getState().add(honey(3))
    first.getState().clear()
    expect(first.getState().lines).toEqual([])
    expect(second.getState().lines).toEqual([honey(3)])
    expect(legacy.getState().lines).toEqual([])
    expect(first.persist.getOptions().name).not.toBe(
      second.persist.getOptions().name
    )
    expect(legacy.persist.getOptions().name).toBe("genesy.cart")
    vi.unstubAllGlobals()
  })
  it("adds a new line, or more of the same product and variant", () => {
    const one = addLine([], honey(1))
    expect(one).toHaveLength(1)
    const three = addLine(one, honey(2))
    expect(three).toEqual([honey(3)])
    expect(addLine(three, honey(1, "v2"))).toHaveLength(2)
  })

  it("keeps quantities whole and within the API's 1 to 1000", () => {
    expect(addLine([honey(999)], honey(5))[0]!.quantity).toBe(1000)
    expect(setQuantity([honey(2)], "p1", "v1", 0)).toEqual([])
    expect(setQuantity([honey(2)], "p1", "v1", 2.7)[0]!.quantity).toBe(2)
    expect(setQuantity([honey(2)], "p1", "v1", 5000)[0]!.quantity).toBe(1000)
  })

  it("stops at 50 lines, the API's limit", () => {
    let lines: CartLine[] = []
    for (let i = 0; i < 55; i++)
      lines = addLine(lines, { ...honey(1), productId: `p${i}` })
    expect(lines).toHaveLength(50)
  })

  it("counts items for the header badge", () => {
    expect(cartCount([honey(2), honey(3, "v2")])).toBe(5)
  })
})
