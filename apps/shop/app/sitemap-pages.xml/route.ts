import { sitemapResponse, xmlLocation } from "@/lib/crawl"
import { isUuid } from "@/lib/passport"
import { siteUrl } from "@/lib/site-origin"
import { listCategories } from "@/lib/storefront"

const publicPages = [
  "/",
  "/shop",
  "/about",
  "/stories",
  "/suppliers",
  "/faq",
  "/contact",
  "/b",
]

export function GET() {
  return sitemapResponse(async () => {
    const categories = (await listCategories(true))
      .filter(({ id, product_count }) => isUuid(id) && product_count > 0)
      .map(({ id }) => `/shop?category=${id}`)
    const entries = [...publicPages, ...categories]
      .map((path) => `<url>${xmlLocation(siteUrl(path))}</url>`)
      .join("\n")
    return `<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${entries}\n</urlset>`
  })
}
