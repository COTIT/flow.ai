import { describe, expect, it } from "vitest"

import { addLine, cartCount, setQuantity, type CartLine } from "./cart"

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
