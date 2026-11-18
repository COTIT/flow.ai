import {
  MAX_PRODUCT_SHARDS,
  SITEMAP_PAGE_SIZE,
  sitemapResponse,
  xmlLocation,
} from "@/lib/crawl"
import { siteUrl } from "@/lib/site-origin"
import { listProducts } from "@/lib/storefront"

export function GET() {
  return sitemapResponse(async () => {
    const { meta } = await listProducts(1, undefined, SITEMAP_PAGE_SIZE)
    const count = Math.ceil(meta.total / SITEMAP_PAGE_SIZE)
    if (!Number.isSafeInteger(count) || count < 0 || count > MAX_PRODUCT_SHARDS)
      throw new Error("Invalid product sitemap size")
    const shards = Array.from(
      { length: count },
      (_, index) => `/sitemap-products-${index + 1}.xml`
    )
    const entries = ["/sitemap-pages.xml", ...shards]
      .map((path) => `<sitemap>${xmlLocation(siteUrl(path))}</sitemap>`)
      .join("\n")
    return `<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${entries}\n</sitemapindex>`
  })
}
