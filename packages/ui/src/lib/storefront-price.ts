import type { StorefrontProduct } from "@genesy/api-client/model"

/** Default variant price matches existing Shop presentation. */
export const defaultVariant = (product: Pick<StorefrontProduct, "variants">) =>
  product.variants.find((v) => v.is_default) ?? product.variants[0] ?? null

/** Variant price wins over product price for size-based products. */
export const shownPrice = (product: StorefrontProduct) =>
  defaultVariant(product)?.price_gross ?? product.price_gross
