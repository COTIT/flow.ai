import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import type { StorefrontProduct } from "@genesy/api-client/model"

import { ProductCard } from "./product-card"
import { StockBadge } from "./stock-badge"

const labels = { inStock: "In stock", soldOut: "Sold out", imageSoon: "Photo" }

const product: StorefrontProduct = {
  id: "p1",
  name: "Ilundo honey",
  brand: "Ilundo",
  type: "physical",
  short_description: "Raw forest honey",
  description: null,
  ean13: null,
  barcode: null,
  price_gross: 1499,
  price_net: 1499,
  currency: "USD",
  currency_decimals: 2,
  available_stock: "7.000",
  stock_status: "in_stock",
  labels: [],
  passport: null,
  images: [],
  variants: [],
}

describe("ProductCard", () => {
  it("links to the product and shows the API's gross price, formatted", () => {
    const html = renderToStaticMarkup(
      <ProductCard product={product} locale="en" labels={labels} />
    )
    expect(html).toContain('href="/products/p1"')
    expect(html).toContain("$14.99")
    expect(html).toContain("Ilundo honey")
  })
})

describe("StockBadge", () => {
  const badge = (stock: string | null) =>
    renderToStaticMarkup(<StockBadge stock={stock} labels={labels} />)

  it("says sold out at zero or below", () => {
    expect(badge("0.000")).toContain("Sold out")
    expect(badge("-1")).toContain("Sold out")
  })

  it("says in stock above zero", () => {
    expect(badge("0.500")).toContain("In stock")
  })

  it("shows nothing when the product isn't stock-tracked", () => {
    expect(badge(null)).toBe("")
  })
})
