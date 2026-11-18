import type { StorefrontProduct } from "@genesy/api-client/model"

import { isSoldOut } from "@/components/stock-badge"
import { shownPrice } from "./price"
import { publicProductImage } from "./product-image"
import { cataloguePath } from "./seo"
import { siteUrl } from "./site-origin"

function offerPrice(product: StorefrontProduct) {
  const minor = shownPrice(product)
  const decimals = product.currency_decimals
  if (!Number.isSafeInteger(minor) || minor < 0) return
  if (!Number.isInteger(decimals) || decimals < 0 || decimals > 8) return
  if (!/^[A-Z]{3}$/.test(product.currency)) return

  const digits = String(minor).padStart(decimals + 1, "0")
  const split = digits.length - decimals
  return decimals ? `${digits.slice(0, split)}.${digits.slice(split)}` : digits
}

function availability(product: StorefrontProduct) {
  if (isSoldOut(product)) return "https://schema.org/OutOfStock"
  if (product.stock_status === "low")
    return "https://schema.org/LimitedAvailability"
  if (
    product.stock_status === "in_stock" ||
    (product.stock_status == null && product.available_stock != null)
  )
    return "https://schema.org/InStock"
}

export function productSchema(product: StorefrontProduct, shopLabel: string) {
  const url = siteUrl(`/products/${product.id}`)
  const image = publicProductImage(product)
  const price = offerPrice(product)
  const crumbs = [
    { name: shopLabel, item: siteUrl("/shop") },
    ...(product.category
      ? [
          {
            name: product.category.name,
            item: siteUrl(cataloguePath(1, product.category.id)),
          },
        ]
      : []),
    { name: product.name, item: url },
  ]

  return {
    "@context": "https://schema.org",
    "@graph": [
      {
        "@type": "Product",
        "@id": `${url}#product`,
        url,
        name: product.name,
        description:
          product.description?.trim() ||
          product.short_description?.trim() ||
          undefined,
        image: image ? siteUrl(image) : undefined,
        category: product.category?.name,
        brand:
          !product.category && product.brand
            ? { "@type": "Brand", name: product.brand }
            : undefined,
        offers:
          price === undefined
            ? undefined
            : {
                "@type": "Offer",
                url,
                price,
                priceCurrency: product.currency,
                availability: availability(product),
              },
      },
      {
        "@type": "BreadcrumbList",
        itemListElement: crumbs.map((crumb, index) => ({
          "@type": "ListItem",
          position: index + 1,
          ...crumb,
        })),
      },
    ],
  }
}
