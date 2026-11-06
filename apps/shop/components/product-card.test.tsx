import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import type { StorefrontProduct } from "@genesy/api-client/model"

import { ProductCard } from "./product-card"
import { StockBadge } from "./stock-badge"

const labels = {
  inStock: "In stock",
  low: "Few left",
  soldOut: "Sold out",
  imageSoon: "Photo",
}

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
  category: null,
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

  it("names the category over the brand, and falls back to the brand", () => {
    const tagged = renderToStaticMarkup(
      <ProductCard
        product={{ ...product, category: { id: "c1", name: "Honey" } }}
        locale="en"
        labels={labels}
      />
    )
    expect(tagged).toContain(">Honey<")
    expect(tagged).not.toContain(">Ilundo<")
    const plain = renderToStaticMarkup(
      <ProductCard product={product} locale="en" labels={labels} />
    )
    expect(plain).toContain(">Ilundo<")
  })

  it("sold by size: shows the default size's price, not the product's 0", () => {
    const size = (id: string, price_gross: number, is_default: boolean) => ({
      id,
      sku: null,
      barcode: null,
      ean13: null,
      is_default,
      price_gross,
      price_net: price_gross,
      options: [],
      images: [],
    })
    const html = renderToStaticMarkup(
      <ProductCard
        product={{
          ...product,
          price_gross: 0,
          variants: [size("v1", 4000, false), size("v2", 1200, true)],
        }}
        locale="en"
        labels={labels}
      />
    )
    expect(html).toContain("$12.00")
    expect(html).not.toContain("$0.00")
  })
})

describe("StockBadge", () => {
  const badge = (stock_status: string | null, available_stock: string | null) =>
    renderToStaticMarkup(
      <StockBadge product={{ stock_status, available_stock }} labels={labels} />
    )

  it("follows the API's stock_status, each in its tone", () => {
    expect(badge("in_stock", "7.000")).toMatch(/data-variant="teal".*In stock/)
    expect(badge("low", "2.000")).toMatch(/data-variant="gold".*Few left/)
    expect(badge("sold_out", "0.000")).toMatch(/data-variant="slate".*Sold out/)
  })

  it("without stock_status, falls back to the available stock", () => {
    expect(badge(null, "0.000")).toContain("Sold out")
    expect(badge(null, "-1")).toContain("Sold out")
    expect(badge(null, "0.500")).toContain("In stock")
  })

  it("shows nothing when the product isn't stock-tracked", () => {
    expect(badge(null, null)).toBe("")
  })
})
