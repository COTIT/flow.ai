import "server-only"

import { cache } from "react"
import { headers } from "next/headers"
import { notFound } from "next/navigation"
import { apiRequest } from "@genesy/api-client/mutator"
import { SHOP_HEADER, shopBase } from "./shop-path"
import "./api"

export const getShopBase = cache(async () =>
  shopBase((await headers()).get(SHOP_HEADER))
)

export const getShop = cache(async () => {
  const base = await getShopBase()
  if (!base) return null
  try {
    const result = await apiRequest<{
      data: { id: string; active: boolean; name: string }
    }>(
      { url: `/storefront/shops/${base.slice(3)}`, method: "GET" },
      { timeout: 8000, adapter: "fetch", fetchOptions: { cache: "no-store" } }
    )
    return result.data
  } catch (error) {
    if ((error as { response?: { status?: number } }).response?.status === 404)
      notFound()
    throw new Error("The shop connection is unavailable", { cause: error })
  }
})
