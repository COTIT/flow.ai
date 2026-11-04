import type { StorefrontProduct } from "@genesy/api-client/model"

/** The size shown first: the API's default variant, else its first; null when none. */
export const defaultVariant = (product: Pick<StorefrontProduct, "variants">) =>
  product.variants.find((v) => v.is_default) ?? product.variants[0] ?? null

/**
 * The price to show before a size is picked. A product sold by size keeps its prices on the
 * variants (its own `price_gross` may be 0), so the default size's price wins.
 */
export const shownPrice = (product: StorefrontProduct) =>
  defaultVariant(product)?.price_gross ?? product.price_gross
