import "server-only"

import { cache } from "react"
import { notFound } from "next/navigation"

import {
  storefrontLotsGet,
  storefrontOrdersCreate,
  storefrontOrdersGet,
  storefrontOrdersPayment,
  storefrontProductsGet,
  storefrontProductsList,
} from "@genesy/api-client/endpoints/storefront"
import { apiRequest } from "@genesy/api-client/mutator"
import type {
  CheckoutRequest,
  StorefrontOrder,
  StorefrontProduct,
} from "@genesy/api-client/model"

import "./api"
import { isUuid, isPublicCode } from "./passport"

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

/**
 * 404 -> Next's not-found page; anything else -> StorefrontUnavailable. The log line holds
 * only the API path and the upstream status: never the key, headers or response body.
 */
async function call<T>(
  path: string,
  request: (options: RequestOptions) => Promise<T>,
  /** What a 404 means instead of not-found, when the page can do without it. */
  missing?: () => T
): Promise<T> {
  const opts = options()
  try {
    return await request(opts)
  } catch (error) {
    const status = (error as { response?: { status?: number } }).response
      ?.status
    if (status === 404 && missing) return missing()
    if (status === 404) notFound()
    console.error(
      `storefront: GET ${path} failed:`,
      status ?? (error as { code?: string }).code ?? "network"
    )
    throw new StorefrontUnavailable()
  }
}

/** One shop category, with how many sellable products it holds. */
// shortcut: hand-typed until GET /storefront/categories is in the generated client; then use its types.
export type StorefrontCategory = {
  id: string
  name: string
  description: string | null
  product_count: number
}
/** A product as the categories API sends it: with its category. */
export type CategorisedProduct = StorefrontProduct & {
  category?: { id: string; name: string } | null
}

/**
 * The shop's categories that have products for sale, by name. An API without the endpoint
 * (404) means none: the shop opens without category tiles rather than as not-found.
 */
export const listCategories = cache(
  async () =>
    (
      await call(
        "/storefront/categories",
        (o) =>
          apiRequest<{ data: StorefrontCategory[] }>(
            { url: "/storefront/categories", method: "GET" },
            o
          ),
        () => ({ data: [] })
      )
    ).data
)

export const listProducts = async (page: number, category?: string) => {
  // The category id goes into the query: only a UUID may.
  if (category && !isUuid(category)) category = undefined
  const list = await call("/storefront/products", (o) =>
    storefrontProductsList(
      // shortcut: `category` is cast in until the generated params know it.
      { page, page_size: PAGE_SIZE, ...(category && { category }) } as {
        page: number
      },
      o
    )
  )
  return list as Omit<typeof list, "data"> & { data: CategorisedProduct[] }
}

/** Per-request memo: generateMetadata and the page share one call. */
export const getProduct = cache(async (id: string) => {
  // The id goes into the API path: anything but a UUID could point it elsewhere.
  if (!isUuid(id)) notFound()
  return (
    await call(`/storefront/products/${id}`, (o) =>
      storefrontProductsGet(id, o)
    )
  ).data as CategorisedProduct
})

/** Orders and batch recalls change by the second: never served from the data cache. */
const fresh = (o: RequestOptions): RequestOptions => ({
  ...o,
  fetchOptions: { cache: "no-store" },
})

/** Published batch passport for a QR code (`/b/{public_code}`); never cached, so a recall shows at once. */
export const getLot = cache(async (code: string) => {
  if (!isPublicCode(code)) notFound()
  return (
    await call(`/storefront/lots/${code}`, (o) =>
      storefrontLotsGet(code, fresh(o))
    )
  ).data
})

const failure = (error: unknown) => {
  const response = (error as { response?: { status?: number; data?: unknown } })
    .response
  return { status: response?.status, data: response?.data }
}

/** Order page: the server's view of the order (no customer data in it). */
export const getOrder = cache(async (id: string) => {
  if (!isUuid(id)) notFound()
  return (
    await call(`/storefront/orders/${id}`, (o) =>
      storefrontOrdersGet(id, fresh(o))
    )
  ).data
})

export type CreateOrderResult =
  | { ok: true; order: StorefrontOrder }
  | { ok: false; kind: "invalid"; errors: Record<string, string[]> }
  | { ok: false; kind: "busy" | "unavailable" }

/**
 * Places the web order. The API prices it from the catalog and holds the stock; its
 * reply is the only source of totals. The idempotency key makes a double submit one order.
 */
export async function createOrder(
  body: CheckoutRequest,
  idempotencyKey: string
): Promise<CreateOrderResult> {
  const o = options()
  try {
    const { data } = await storefrontOrdersCreate(body, {
      ...fresh(o),
      headers: { ...(o.headers as object), "Idempotency-Key": idempotencyKey },
    })
    return { ok: true, order: data }
  } catch (error) {
    const { status, data } = failure(error)
    const errors = (data as { errors?: Record<string, string[]> })?.errors
    if (status === 422 && errors) return { ok: false, kind: "invalid", errors }
    if (status === 429) return { ok: false, kind: "busy" }
    console.error(
      "storefront: POST /storefront/orders failed:",
      status ?? "network"
    )
    return { ok: false, kind: "unavailable" }
  }
}

export type PaymentResult =
  | { ok: true; checkoutUrl: string }
  | {
      ok: false
      kind: "no_provider" | "provider_down" | "not_payable" | "unavailable"
    }

/** Starts (or resumes) the payment; the provider sends the customer back to `redirectUrl`. */
export async function startPayment(
  orderId: string,
  redirectUrl: string
): Promise<PaymentResult> {
  const path = `/storefront/orders/${orderId}/payment`
  try {
    const { data } = await storefrontOrdersPayment(
      orderId,
      { redirect_url: redirectUrl },
      fresh(options())
    )
    return data.checkout_url
      ? { ok: true, checkoutUrl: data.checkout_url }
      : { ok: false, kind: "not_payable" }
  } catch (error) {
    if (error instanceof StorefrontUnavailable)
      return { ok: false, kind: "unavailable" }
    const { status, data } = failure(error)
    const message = (data as { message?: string })?.message
    if (status === 503)
      return {
        ok: false,
        kind:
          message === "no payment provider configured"
            ? "no_provider"
            : "provider_down",
      }
    if (status === 409) return { ok: false, kind: "not_payable" }
    console.error(`storefront: POST ${path} failed:`, status ?? "network")
    return { ok: false, kind: "unavailable" }
  }
}
