import type { StorefrontProduct } from "@verzo/api-client/model"

import { isUuid } from "./passport"

type ProductImage = Pick<StorefrontProduct, "id" | "images">
const rasterTypes = new Set([
  "image/png",
  "image/jpeg",
  "image/gif",
  "image/webp",
])

/** Storage signs the MIME override with the upload's validated content type. */
export function primaryImageSource(product: ProductImage) {
  const source = product.images?.[0]?.url
  const origin = process.env.FILES_ORIGIN
  if (!source || !origin || !isUuid(product.id)) return
  try {
    const url = new URL(source)
    const allowed = new URL(origin)
    const mime = url.searchParams.get("response-content-type")
    if (
      (url.protocol === "https:" || url.protocol === "http:") &&
      url.origin === allowed.origin &&
      !url.username &&
      !url.password &&
      mime &&
      rasterTypes.has(mime)
    )
      return { url, mime }
  } catch {
    return
  }
}

export const publicProductImage = (product: ProductImage) =>
  primaryImageSource(product) ? `/products/${product.id}/image` : undefined
