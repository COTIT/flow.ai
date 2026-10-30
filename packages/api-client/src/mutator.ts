import type { AxiosError, AxiosInstance, AxiosRequestConfig } from "axios"

import { createApiClient, type ApiClientOptions } from "./client"

let client: AxiosInstance | null = null

/** Points every generated call at this app's backend. Call once, at app startup. */
export function configureApi(options: ApiClientOptions): AxiosInstance {
  client = createApiClient(options)
  return client
}

/** orval mutator: every generated function sends through here and gets the body back. */
export async function apiRequest<T>(
  config: AxiosRequestConfig,
  options?: AxiosRequestConfig
): Promise<T> {
  if (!client) throw new Error("API not configured: call configureApi() at app startup")
  const { data } = await client.request<T>({ ...config, ...options })
  return data
}

/** Error type orval puts on generated hooks. */
export type ErrorType<E> = AxiosError<E>
/** Request body type orval puts on generated calls. */
export type BodyType<B> = B
