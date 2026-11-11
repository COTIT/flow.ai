import "server-only"

import type {
  StorefrontPreviewDesign,
  StorefrontPublishedDesign,
} from "@genesy/api-client/model"
import { apiRequest } from "@genesy/api-client/mutator"
import { canRenderStorefrontPage } from "@genesy/ui/lib/storefront-design"

import "./api"

/** Short fixed reason only: never payloads, headers, keys or error text. */
const legacy = (reason: string) => {
  console.warn("storefront: using legacy home:", reason)
  return null
}

/** Design freshness stays separate from existing catalog cache contracts. */
export async function getPublishedDesign(): Promise<StorefrontPublishedDesign | null> {
  if (process.env.SHOP_DESIGN_ENABLED !== "true") return null
  const key = process.env.STOREFRONT_APP_KEY
  if (!key) return legacy("no app key")
  try {
    const { data } = await apiRequest<{
      data: StorefrontPublishedDesign | null
    }>(
      { url: "/storefront/pages/home", method: "GET" },
      {
        headers: { Authorization: `Bearer ${key}` },
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

/** Every real preview token: 64 lowercase hex characters (API `auth::token`). */
const TOKEN = /^[0-9a-f]{64}$/

/**
 * Unpublished draft behind a preview link, or null for any failure (expired, revoked, unknown,
 * unsupported). The token travels in the request body only and is never logged.
 */
export async function getPreviewDesign(
  token: string
): Promise<StorefrontPreviewDesign | null> {
  if (!TOKEN.test(token)) return null
  const key = process.env.STOREFRONT_APP_KEY
  if (!key) return null
  try {
    const { data } = await apiRequest<{ data: StorefrontPreviewDesign }>(
      { url: "/storefront/preview", method: "POST", data: { token } },
      {
        headers: { Authorization: `Bearer ${key}` },
        timeout: 2000,
        adapter: "fetch",
        fetchOptions: { cache: "no-store" },
      }
    )
    return canRenderStorefrontPage(data.document, data.collections)
      ? data
      : null
  } catch (error) {
    const status = (error as { response?: { status?: number } })?.response
      ?.status
    // Short fixed reason only, like `legacy`: never the token, URL or error text.
    console.warn(
      "storefront: preview unavailable:",
      typeof status === "number" ? `HTTP ${status}` : "network"
    )
    return null
  }
}
