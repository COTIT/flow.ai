// Hand-written calls for routes the OpenAPI spec doesn't describe yet.
// Shapes come from apps/api/docs/api/products.md. Delete each block once
// its generated twin exists, and point callers at the generated hook.

import { useQuery } from "@tanstack/react-query"
import type { AxiosRequestConfig } from "axios"

import type { Meta } from "./generated/model"
import { apiRequest } from "./mutator"

/** The product fields the office list shows. Prices are strings, printed by the API. */
export type ProductRow = {
  id: string
  name: string
  product_nr: string | null
  reference: string | null
  price_incl: string
  status: string
}

export type ProductsListParams = { page?: number; q?: string }

// TODO(openapi group C): replace with the generated `GET /workspace/{workspace}/products` hook.
export const productsList = (
  workspace: string,
  params?: ProductsListParams,
  options?: AxiosRequestConfig,
  signal?: AbortSignal
) =>
  apiRequest<{ data: ProductRow[]; meta: Meta }>(
    { url: `/workspace/${workspace}/products`, method: "GET", params, signal },
    options
  )

export const useProductsList = (
  workspace: string | null,
  params?: ProductsListParams
) =>
  useQuery({
    queryKey: [`/workspace/${workspace}/products`, params],
    queryFn: ({ signal }) =>
      productsList(workspace!, params, undefined, signal),
    enabled: Boolean(workspace),
  })
