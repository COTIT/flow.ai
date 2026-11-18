import type { StorefrontProduct } from "@genesy/api-client/model"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { productSchema } from "./product-schema"

const product: StorefrontProduct = {
  id: "c1000000-0000-4000-8000-000000000002",
  name: "Coffee",
  brand: null,
  type: "physical",
  short_description: "Coffee grown in East Africa",
  description: "A bag of coffee",
  ean13: null,
  barcode: null,
  price_gross: 750,
  price_net: 500,
  currency: "USD",
  currency_decimals: 2,
  available_stock: "20",
  stock_status: "in_stock",
  labels: [],
  category: { id: "c2000000-0000-4000-8000-000000000002", name: "Pantry" },
  passport: null,
  images: [],
  variants: [],
}

const primaryPhoto = {
  url: "https://files.example/product.jpg?response-content-type=image%2Fjpeg&signature=short-lived",
  expires_at: "2030-01-01T00:00:00Z",
}

beforeEach(() => {
  vi.stubEnv("SITE_URL", "https://shop.example")
  vi.stubEnv("FILES_ORIGIN", "https://files.example")
})

describe("product structured data", () => {
  it("matches the visible default-variant offer and uses only stable URLs", () => {
    const data = productSchema(
      {
        ...product,
        stock_status: "low",
        images: [primaryPhoto],
        variants: [
          {
            id: "c3000000-0000-4000-8000-000000000002",
            is_default: true,
            sku: null,
            barcode: null,
            ean13: null,
            price_gross: 1299,
            price_net: 1100,
            options: [],
            images: [],
          },
        ],
      },
      "Shop"
    )
    const url = `https://shop.example/products/${product.id}`
    expect(data["@graph"]).toMatchObject([
      {
        "@type": "Product",
        image: `${url}/image`,
        offers: {
          "@type": "Offer",
          url,
          price: "12.99",
          priceCurrency: "USD",
          availability: "https://schema.org/LimitedAvailability",
        },
      },
      {
        "@type": "BreadcrumbList",
        itemListElement: [
          { position: 1, item: "https://shop.example/shop" },
          {
            position: 2,
            item: `https://shop.example/shop?category=${product.category!.id}`,
          },
          { position: 3, item: url },
        ],
      },
    ])
    expect(JSON.stringify(data)).not.toContain(primaryPhoto.url)
  })

  it.each([
    [{ currency_decimals: 0, price_gross: 800 }, "800"],
    [{ currency_decimals: 3, price_gross: 8 }, "0.008"],
    [{ currency_decimals: 8, price_gross: 12 }, "0.00000012"],
  ])("serializes currency precision exactly for %j", (values, price) => {
    expect(
      productSchema({ ...product, ...values }, "Shop")["@graph"][0]
    ).toMatchObject({
      offers: { price },
    })
  })

  it("handles sold out and missing availability without guessing", () => {
    const soldOut = productSchema(
      { ...product, stock_status: "sold_out" },
      "Shop"
    )
    expect(soldOut["@graph"][0]).toMatchObject({
      offers: { availability: "https://schema.org/OutOfStock" },
    })
    const untracked = productSchema(
      { ...product, stock_status: null, available_stock: null },
      "Shop"
    )
    expect(JSON.stringify(untracked)).not.toContain("availability")
  })

  it("omits untrusted photos and unsafe prices", () => {
    const data = productSchema(
      {
        ...product,
        price_gross: Number.MAX_SAFE_INTEGER + 1,
        images: [{ ...primaryPhoto, url: "https://other.example/product.jpg" }],
      },
      "Shop"
    )
    expect(JSON.stringify(data)).not.toContain('"image"')
    expect(JSON.stringify(data)).not.toContain('"offers"')
  })
})
