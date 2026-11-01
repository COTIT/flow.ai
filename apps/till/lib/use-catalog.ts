"use client"

import { useQuery } from "@tanstack/react-query"

import { storefrontProductsList } from "@genesy/api-client/endpoints/storefront"
import type { StorefrontProduct } from "@genesy/api-client/model"
import { deviceAuth, useTill, type Device } from "@/lib/till-store"

/**
 * The till's catalog, in one place so the coming till catalog API is a one-file swap.
 * Today: every page of `/storefront/products`, read with the device key (`catalog:read`).
 */
async function loadAll(device: Device): Promise<StorefrontProduct[]> {
  const all: StorefrontProduct[] = []
  for (let page = 1; ; page++) {
    const res = await storefrontProductsList(
      { page, page_size: 100 },
      deviceAuth(device)
    )
    all.push(...res.data)
    if (page >= res.meta.last_page) return all
  }
}

/** One thing the till can sell: a product, or one variant of it. */
export type Sellable = {
  productId: string
  variantId: string | null
  name: string
  unitNet: number
  unitGross: number
  codes: string[]
}

function sellables(products: StorefrontProduct[]): Sellable[] {
  return products.flatMap<Sellable>((p) =>
    p.variants.length
      ? p.variants.map((v) => ({
          productId: p.id,
          variantId: v.id,
          name: [p.name, ...v.options.map((o) => o.value)].join(" · "),
          unitNet: v.price_net,
          unitGross: v.price_gross,
          codes: [v.ean13, v.barcode, v.sku].filter((c): c is string => !!c),
        }))
      : [
          {
            productId: p.id,
            variantId: null,
            name: p.name,
            unitNet: p.price_net,
            unitGross: p.price_gross,
            codes: [p.ean13, p.barcode].filter((c): c is string => !!c),
          },
        ]
  )
}

export function useTillCatalog() {
  const device = useTill((s) => s.device)
  const query = useQuery({
    queryKey: ["till-catalog", device?.workspaceId],
    queryFn: () => loadAll(device!),
    enabled: Boolean(device),
    staleTime: 5 * 60_000,
  })
  const products = query.data ?? []
  const first = products[0]
  const items = sellables(products)
  return {
    ...query,
    products,
    /** Scanned or typed code: exact match on EAN, barcode or SKU. */
    findByCode: (code: string) =>
      items.find((i) => i.codes.includes(code.trim())) ?? null,
    /** Name or code containing the text, case-insensitive; display filter only. */
    search: (text: string) => {
      const q = text.trim().toLowerCase()
      return q
        ? items.filter(
            (i) =>
              i.name.toLowerCase().includes(q) ||
              i.codes.some((c) => c.toLowerCase().includes(q))
          )
        : []
    },
    /** shortcut: the catalog's currency until `/till/me` carries the register's own. */
    currency: first
      ? { code: first.currency, decimals: first.currency_decimals }
      : null,
  }
}
