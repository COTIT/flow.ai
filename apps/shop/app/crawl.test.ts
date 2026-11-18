import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { GET as productImage } from "./products/[id]/image/route"
import robots from "./robots"
import { GET as sitemapPages } from "./sitemap-pages.xml/route"
import { GET as sitemapProducts } from "./sitemap-products/[page]/route"
import { GET as sitemapIndex } from "./sitemap.xml/route"

const { getProduct, getShopBase, listCategories, listProducts } = vi.hoisted(
  () => ({
    getProduct: vi.fn(),
    getShopBase: vi.fn(),
    listCategories: vi.fn(),
    listProducts: vi.fn(),
  })
)
vi.mock("@/lib/storefront", () => ({
  getProduct,
  listCategories,
  listProducts,
}))
vi.mock("@/lib/shop-scope", () => ({ getShopBase }))

const site = "https://shop.example"
const files = "https://bucket.example"
const id = "11111111-1111-4111-8111-111111111111"
const otherId = "22222222-2222-4222-8222-222222222222"
const path = `/products/${id}/image`
const source = `${files}/honey?response-content-type=image%2Fpng&X-Amz-Signature=secret`
const product = { id, images: [{ url: source }] }
const requestImage = (productId = id) =>
  productImage(
    new Request(
      `${site}/products/${productId}/image?url=https://untrusted.example`,
      {
        headers: { Cookie: "session=private", Authorization: "Bearer private" },
      }
    ),
    { params: Promise.resolve({ id: productId }) }
  )
const requestShard = (page: string) =>
  sitemapProducts(new Request(`${site}/sitemap-products-${page}.xml`), {
    params: Promise.resolve({ page }),
  })
const png = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jqskAAAAASUVORK5CYII=",
  "base64"
)

beforeEach(() => {
  vi.stubEnv("SITE_URL", site)
  vi.stubEnv("FILES_ORIGIN", files)
  vi.stubEnv("NOINDEX", "")
  getShopBase.mockResolvedValue("")
})

afterEach(() => {
  vi.resetAllMocks()
  vi.unstubAllEnvs()
  vi.unstubAllGlobals()
})

describe("storefront sitemap", () => {
  it("discovers root-level shards with one bounded catalog read", async () => {
    listProducts.mockResolvedValue({ meta: { total: 201 } })
    const response = await sitemapIndex()
    const xml = await response.text()
    expect(response.headers.get("Content-Type")).toContain("application/xml")
    expect(response.headers.get("Cache-Control")).toContain("s-maxage=60")
    expect(xml).toContain("<sitemapindex")
    for (const page of ["pages", "products-1", "products-2", "products-3"])
      expect(xml).toContain(`<loc>${site}/sitemap-${page}.xml</loc>`)
    expect(xml).not.toContain("products-4")
    expect(listProducts).toHaveBeenCalledExactlyOnceWith(1, undefined, 100)
  })

  it("discovers populated categories and public pages", async () => {
    listCategories.mockResolvedValue([
      { id, product_count: 1 },
      { id: otherId, product_count: 0 },
    ])
    const xml = await (await sitemapPages()).text()
    for (const path of [
      "/",
      "/shop",
      "/about",
      "/stories",
      "/suppliers",
      "/faq",
      "/contact",
      "/b",
      `/shop?category=${id}`,
    ])
      expect(xml).toContain(`<loc>${site}${path}</loc>`)
    expect(xml).not.toMatch(
      /\/cart|\/orders|\/checkout|\/preview|\/s\/|\?page|22222222/
    )
    expect(listCategories).toHaveBeenCalledExactlyOnceWith(true)
  })

  it("publishes stable primary raster image URLs only for its product shard", async () => {
    listProducts.mockResolvedValue({
      data: [
        { ...product, stock_status: "sold_out" },
        {
          id: otherId,
          images: [
            { url: source.replace("image%2Fpng", "image%2Fsvg%2Bxml") },
            { url: source },
          ],
        },
      ],
    })
    const xml = await (await requestShard("2")).text()
    expect(xml).toContain("http://www.google.com/schemas/sitemap-image/1.1")
    expect(xml).toContain(`<image:loc>${site}${path}</image:loc>`)
    expect(xml).toContain(`<loc>${site}/products/${otherId}</loc>`)
    expect(xml).not.toContain(`/products/${otherId}/image`)
    expect(xml).not.toMatch(/X-Amz-Signature|bucket\.example/)
    expect(listProducts).toHaveBeenCalledExactlyOnceWith(2, undefined, 100)
    expect(getProduct).not.toHaveBeenCalled()
  })

  it("serves retryable outages without partial XML", async () => {
    listProducts.mockRejectedValue(new Error("API unavailable"))
    listCategories.mockRejectedValue(new Error("Categories unavailable"))
    for (const response of [
      await sitemapIndex(),
      await sitemapPages(),
      await requestShard("1"),
    ]) {
      expect(response.status).toBe(503)
      expect(response.headers.get("Retry-After")).toBe("60")
      expect(response.headers.get("Cache-Control")).toBe("no-store")
      expect(await response.text()).toBe("")
    }
  })

  it("suppresses discovery and catalog reads on staging", async () => {
    expect(robots().sitemap).toEqual(`${site}/sitemap.xml`)
    vi.stubEnv("NOINDEX", "1")
    expect(robots()).toEqual({ rules: { userAgent: "*", disallow: "/" } })
    for (const response of [
      await sitemapIndex(),
      await sitemapPages(),
      await requestShard("1"),
    ])
      expect(response.status).toBe(404)
    expect(listProducts).not.toHaveBeenCalled()
    expect(listCategories).not.toHaveBeenCalled()
  })

  it("rejects invalid shard numbers before reading the catalog", async () => {
    for (const page of ["0", "01", "1.1", "50000"])
      expect((await requestShard(page)).status).toBe(404)
    expect(listProducts).not.toHaveBeenCalled()
  })
})

describe("public product image", () => {
  it("streams a raster without forwarding credentials or redirecting", async () => {
    getProduct.mockResolvedValue(product)
    const fetchImage = vi.fn().mockResolvedValue(
      new Response(
        new ReadableStream({
          start(controller) {
            controller.enqueue(png.subarray(0, 5))
            controller.enqueue(png.subarray(5))
            controller.close()
          },
        }),
        {
          headers: {
            "Content-Type": "image/png",
            "Set-Cookie": "upstream=private",
          },
        }
      )
    )
    vi.stubGlobal("fetch", fetchImage)

    const response = await requestImage()
    expect(response.status).toBe(200)
    expect(response.headers.get("Content-Type")).toBe("image/png")
    expect(response.headers.get("Set-Cookie")).toBeNull()
    expect(Buffer.from(await response.arrayBuffer())).toEqual(png)
    expect(getProduct).toHaveBeenCalledExactlyOnceWith(id)
    const [url, init] = fetchImage.mock.calls[0]!
    expect(String(url)).toBe(source)
    expect(init).toMatchObject({
      credentials: "omit",
      redirect: "error",
      signal: expect.any(AbortSignal),
    })
    expect(init.headers).toBeUndefined()
  })

  it("rejects invalid or scoped products before reading and untrusted photos before fetching", async () => {
    const fetchImage = vi.fn()
    vi.stubGlobal("fetch", fetchImage)
    expect((await requestImage("not-a-uuid")).status).toBe(404)
    getShopBase.mockResolvedValueOnce("/s/tenant")
    expect((await requestImage()).status).toBe(404)
    expect(getProduct).not.toHaveBeenCalled()
    for (const url of [
      undefined,
      source.replace("bucket.example", "bucket.example.evil"),
      source.replace("bucket.example", "user:password@bucket.example"),
      source.replace("image%2Fpng", "image%2Fsvg%2Bxml"),
    ]) {
      getProduct.mockResolvedValue({ id, images: url ? [{ url }] : [] })
      expect((await requestImage()).status).toBe(404)
    }
    expect(fetchImage).not.toHaveBeenCalled()
  })

  it("rejects non-raster bytes and distinguishes missing files from temporary failures", async () => {
    getProduct.mockResolvedValue(product)
    const fetchImage = vi
      .fn()
      .mockResolvedValueOnce(
        new Response("<svg>spoofed</svg>", {
          headers: { "Content-Type": "image/png" },
        })
      )
      .mockResolvedValueOnce(new Response(null, { status: 404 }))
      .mockResolvedValueOnce(
        new Response(null, {
          status: 302,
          headers: { Location: "https://untrusted.example" },
        })
      )
      .mockRejectedValueOnce(new Error("Network timeout"))
    vi.stubGlobal("fetch", fetchImage)
    expect((await requestImage()).status).toBe(404)
    expect((await requestImage()).status).toBe(404)
    expect((await requestImage()).status).toBe(503)
    const timeout = await requestImage()
    expect(timeout.status).toBe(503)
    expect(timeout.headers.get("Retry-After")).toBe("60")
  })
})
