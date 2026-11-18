import {
  MAX_PRODUCT_SHARDS,
  missingResponse,
  SITEMAP_PAGE_SIZE,
  sitemapResponse,
  xmlImage,
  xmlLocation,
} from "@/lib/crawl"
import { isUuid } from "@/lib/passport"
import { publicProductImage } from "@/lib/product-image"
import { siteUrl } from "@/lib/site-origin"
import { listProducts } from "@/lib/storefront"

export async function GET(
  _request: Request,
  { params }: { params: Promise<{ page: string }> }
) {
  const { page } = await params
  const index = Number(page)
  if (!/^[1-9]\d*$/.test(page) || index > MAX_PRODUCT_SHARDS)
    return missingResponse()

  return sitemapResponse(async () => {
    const { data } = await listProducts(index, undefined, SITEMAP_PAGE_SIZE)
    const entries = data
      .filter(({ id }) => isUuid(id))
      .map((product) => {
        const image = publicProductImage(product)
        const location = xmlLocation(siteUrl(`/products/${product.id}`))
        return `<url>${location}${image ? xmlImage(siteUrl(image)) : ""}</url>`
      })
      .join("\n")
    return `<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:image="http://www.google.com/schemas/sitemap-image/1.1">\n${entries}\n</urlset>`
  })
}
