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
  return {
    ...query,
    products,
    /** shortcut: the catalog's currency until `/till/me` carries the register's own. */
    currency: first
      ? { code: first.currency, decimals: first.currency_decimals }
      : null,
  }
}
