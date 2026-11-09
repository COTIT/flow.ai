import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import type { StorefrontProduct } from "@genesy/api-client/model"

import document from "../../../../apps/api/tests/fixtures/storefront-design-v1.json"
import { canRenderStorefrontPage } from "../lib/storefront-design"
import { StorefrontPage } from "./storefront-page"
import type { StorefrontPageProps } from "./storefront-page"

const product: StorefrontProduct = {
  id: "f1000000-0000-4000-8000-000000000001",
  name: "Forest honey",
  brand: null,
  category: null,
  type: "physical",
  description: null,
  barcode: null,
  ean13: null,
  labels: [],
  passport: null,
  short_description: "Raw honey",
  price_gross: 0,
  price_net: 0,
  currency: "USD",
  currency_decimals: 2,
  stock_status: "low",
  available_stock: "2.000",
  images: [],
  variants: [
    {
      id: "v1",
      sku: null,
      barcode: null,
      ean13: null,
      options: [],
      images: [],
      is_default: false,
      price_gross: 4000,
      price_net: 4000,
    },
    {
      id: "v2",
      sku: null,
      barcode: null,
      ean13: null,
      options: [],
      images: [],
      is_default: true,
      price_gross: 1200,
      price_net: 1200,
    },
  ],
}
const collections = [{ section_id: "featured-products", products: [product] }]
const labels = {
  inStock: "In stock",
  low: "Few left",
  soldOut: "Sold out",
  imageSoon: "Photo coming soon",
  emptyProducts: "No products available",
  shopCta: "Shop products",
}
const props = {
  document,
  collections,
  locale: "en",
  labels,
} as StorefrontPageProps

describe("storefront page", () => {
  it("renders the shared API fixture with authoritative variant pricing and stock", () => {
    expect(canRenderStorefrontPage(document, collections)).toBe(true)
    const html = renderToStaticMarkup(<StorefrontPage {...props} />)
    expect(html).toContain("From farm to your table")
    expect(html).toContain('href="/shop"')
    expect(html).toContain(`href="/products/${product.id}"`)
    expect(html).toContain("$12.00")
    expect(html).not.toContain("$0.00")
    expect(html).toContain("Few left")
    expect(html).toContain("md:grid-cols-2")
    expect(html).toContain("lg:grid-cols-3")
    expect(html.match(/<h1\b/g)).toHaveLength(1)
  })

  it("keeps Office preview inert", () => {
    const html = renderToStaticMarkup(<StorefrontPage {...props} preview />)
    expect(html).not.toContain("href=")
    expect(html).toContain("Forest honey")
    // Office already owns h1 (page) and h2 ("Storefront preview").
    expect(html).not.toContain("<h1")
    expect(html).not.toContain("<h2")
    expect(html.match(/<h3\b/g)).toHaveLength(document.sections.length)
    expect(html).toMatch(/<h4[^>]*>Forest honey<\/h4>/)
  })

  it("keeps a valid heading hierarchy when a document has no hero", () => {
    const withoutHero = {
      ...document,
      sections: document.sections.filter((section) => section.type !== "hero"),
    }
    const html = renderToStaticMarkup(
      <StorefrontPage
        {...props}
        document={withoutHero as StorefrontPageProps["document"]}
      />
    )
    expect(html).toContain("<h1")
    expect(html).toContain("<h2")
    expect(html).not.toContain("<h3")
  })

  it("keeps one leading h1 when sections are reordered", () => {
    const reordered = {
      ...document,
      sections: [...document.sections].reverse(),
    }
    const html = renderToStaticMarkup(
      <StorefrontPage
        {...props}
        document={reordered as StorefrontPageProps["document"]}
      />
    )
    expect(html).toMatch(/<h1[^>]*>Fresh from our growers<\/h1>/)
    expect(html.match(/<h1\b/g)).toHaveLength(1)
  })

  it("escapes document text and gives empty collections a clear state", () => {
    const changed = structuredClone(document)
    changed.sections[0]!.props.heading = '<script>alert("x")</script>'
    const html = renderToStaticMarkup(
      <StorefrontPage
        {...props}
        document={changed as StorefrontPageProps["document"]}
        collections={[{ section_id: "featured-products", products: [] }]}
      />
    )
    expect(html).not.toContain("<script>")
    expect(html).toContain("&lt;script&gt;")
    expect(html).toContain("No products available")
  })

  it("accepts Unicode headings within Rust's character limit", () => {
    const changed = structuredClone(document)
    changed.sections[0]!.props.heading = "🌱".repeat(160)
    expect(canRenderStorefrontPage(changed, collections)).toBe(true)
  })

  it("rejects the whole page before rendering unsupported content", () => {
    const changed = structuredClone(document)
    changed.sections.push({ id: "unsafe", type: "script", props: {} } as never)
    expect(canRenderStorefrontPage(changed, collections)).toBe(false)
    expect(
      renderToStaticMarkup(
        <StorefrontPage
          {...props}
          document={changed as StorefrontPageProps["document"]}
        />
      )
    ).toBe("")
  })

  it.each([
    { ...document, schema_version: 2 },
    { ...document, theme: "url(https://evil.test)" },
    { ...document, sections: [] },
    { ...document, sections: [{ ...document.sections[0], props: null }] },
    {
      ...document,
      sections: [
        {
          ...document.sections[1],
          props: {
            ...document.sections[1]!.props,
            columns: { mobile: 999, tablet: 2, desktop: 3 },
          },
        },
      ],
    },
  ])("refuses renderer-incompatible documents", (value) => {
    expect(canRenderStorefrontPage(value, collections)).toBe(false)
  })

  it("refuses missing/duplicate collections and unsafe product data before render", () => {
    expect(canRenderStorefrontPage(document, [])).toBe(false)
    expect(
      canRenderStorefrontPage(document, [...collections, ...collections])
    ).toBe(false)
    for (const override of [
      { id: "../orders" },
      { price_gross: 0.5 },
      { currency: "bad currency" },
      { images: [{ url: "javascript:alert(1)" }] },
    ]) {
      expect(
        canRenderStorefrontPage(document, [
          {
            section_id: "featured-products",
            products: [{ ...product, ...override }],
          },
        ])
      ).toBe(false)
    }
  })
})
