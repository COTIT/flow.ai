import "server-only"

import { cache } from "react"
import { notFound } from "next/navigation"

import {
  storefrontProductsGet,
  storefrontProductsList,
} from "@genesy/api-client/endpoints/storefront"

import "./api"

/** Axios request options, as the generated calls take them. */
type RequestOptions = NonNullable<Parameters<typeof storefrontProductsGet>[1]>

/** The storefront API is down, slow or refusing the key; pages show the error page. */
export class StorefrontUnavailable extends Error {
  constructor() {
    super("The shop catalog is unavailable")
    this.name = "StorefrontUnavailable"
  }
}

export const PAGE_SIZE = 24

/**
 * Request options for every storefront call. The app key is a server secret: this module
 * is server-only, and the build checks no client file mentions it.
 */
function options(): RequestOptions {
  const key = process.env.STOREFRONT_APP_KEY
  if (!key) {
    console.error("storefront: STOREFRONT_APP_KEY is not set")
    throw new StorefrontUnavailable()
  }
  return {
    headers: { Authorization: `Bearer ${key}` },
    timeout: 8000,
    // Fetch adapter so Next's data cache holds replies; 60 s stays well under the
    // 10-minute life of the signed file links inside them.
    adapter: "fetch",
    fetchOptions: { next: { revalidate: 60 } },
  }
}

/** 404 -> Next's not-found page; anything else -> StorefrontUnavailable (status only is logged). */
async function call<T>(
  request: (options: RequestOptions) => Promise<T>
): Promise<T> {
  const opts = options()
  try {
    return await request(opts)
  } catch (error) {
    const status = (error as { response?: { status?: number } }).response
      ?.status
    if (status === 404) notFound()
    console.error(
      "storefront: call failed",
      status ?? (error as { code?: string }).code ?? "network"
    )
    throw new StorefrontUnavailable()
  }
}

export const listProducts = (page: number) =>
  call((o) => storefrontProductsList({ page, page_size: PAGE_SIZE }, o))

/** Per-request memo: generateMetadata and the page share one call. */
export const getProduct = cache((id: string) =>
  call((o) => storefrontProductsGet(id, o)).then((r) => r.data)
)
