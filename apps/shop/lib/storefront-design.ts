import "server-only"

import type { StorefrontPublishedDesign } from "@genesy/api-client/model"
import { apiRequest } from "@genesy/api-client/mutator"
import { canRenderStorefrontPage } from "@genesy/ui/lib/storefront-design"

import "./api"
import { getShopBase } from "./shop-scope"

/** Short fixed reason only: never payloads, headers, keys or error text. */
const legacy = (reason: string) => {
  console.warn("storefront: using legacy home:", reason)
  return null
}

/** Design freshness stays separate from existing catalog cache contracts. */
export async function getPublishedDesign(): Promise<StorefrontPublishedDesign | null> {
  const base = await getShopBase()
  if (!base && process.env.SHOP_DESIGN_ENABLED !== "true") return null
  const key = process.env.STOREFRONT_APP_KEY
  if (!base && !key) return legacy("no app key")
  try {
    const { data } = await apiRequest<{
      data: StorefrontPublishedDesign | null
    }>(
      {
        url: base
          ? `/storefront/shops/${base.slice(3)}/pages/home`
          : "/storefront/pages/home",
        method: "GET",
      },
      {
        ...(base ? {} : { headers: { Authorization: `Bearer ${key}` } }),
        timeout: 2000,
        adapter: "fetch",
        fetchOptions: { cache: "no-store" },
      }
    )
    if (!data) return null // nothing published yet: normal, not a failure
    return canRenderStorefrontPage(data.document, data.collections)
      ? data
      : legacy("unsupported design")
  } catch (error) {
    const status = (error as { response?: { status?: number } })?.response
      ?.status
    return legacy(typeof status === "number" ? `HTTP ${status}` : "network")
  }
}
