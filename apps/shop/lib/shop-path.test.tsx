import { describe, expect, it } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import ShopLink from "@/components/shop-link"
import { ShopContext } from "./shop-context"
import { shopBase, shopPath } from "./shop-path"

const base = "/s/00000000-0000-4000-8000-000000000001"
describe("workspace shop links", () => {
  it("accepts UUID scope only and preserves legacy URLs", () => {
    expect(shopBase("../../private")).toBe("")
    expect(shopBase(base.slice(3))).toBe(base)
    expect(shopPath("", "/products/one")).toBe("/products/one")
    expect(shopPath(base, "/")).toBe(base)
    expect(shopPath(base, "/shop?page=2")).toBe(`${base}/shop?page=2`)
    expect(shopPath(base, `${base}/cart`)).toBe(`${base}/cart`)
    expect(shopPath(base, "https://provider.test/pay")).toBe(
      "https://provider.test/pay"
    )
    expect(shopPath(base, "//provider.test/pay")).toBe("//provider.test/pay")
  })
  it("keeps rendered product, cart and order links inside the same workspace", () => {
    const html = renderToStaticMarkup(
      <ShopContext value={{ base, name: "Test" }}>
        <ShopLink href="/products/p1">Product</ShopLink>
        <ShopLink href="/cart">Cart</ShopLink>
        <ShopLink href="/orders/o1">Order</ShopLink>
      </ShopContext>
    )
    for (const path of ["/products/p1", "/cart", "/orders/o1"])
      expect(html).toContain(`href="${base}${path}"`)
  })
})
