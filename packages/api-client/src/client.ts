import axios, { type AxiosInstance } from "axios"

/** Settings for one app's connection to the Genesy backend. */
export type ApiClientOptions = {
  /** Backend address, e.g. `http://localhost:8080/v1`. */
  baseURL: string
  /** Returns the current login token, or `null` when logged out. Read on every request. */
  getToken?: () => string | null
  /** Called when the backend answers 401 (token expired or revoked), e.g. to log out. */
  onUnauthorized?: () => void
}

/** Axios instance for the Genesy backend: JSON in and out, bearer token when logged in. */
export function createApiClient({
  baseURL,
  getToken,
  onUnauthorized,
}: ApiClientOptions): AxiosInstance {
  const api = axios.create({
    baseURL,
    headers: { Accept: "application/json" },
  })

  api.interceptors.request.use((config) => {
    const token = getToken?.()
    // A call that brings its own credential (e.g. the till's device key) keeps it.
    if (token && !config.headers.has("Authorization"))
      config.headers.set("Authorization", `Bearer ${token}`)
    return config
  })

  api.interceptors.response.use(undefined, (error) => {
    if (axios.isAxiosError(error) && error.response?.status === 401)
      onUnauthorized?.()
    return Promise.reject(error)
  })

  return api
}
