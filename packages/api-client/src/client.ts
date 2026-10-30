import axios, { type AxiosInstance } from "axios"

/** Settings for one app's connection to the Genesy backend. */
export type ApiClientOptions = {
  /** Backend address, e.g. `http://localhost:8080/v1`. */
  baseURL: string
  /** Returns the current login token, or `null` when logged out. Read on every request. */
  getToken?: () => string | null
}

/** Axios instance for the Genesy backend: JSON in and out, bearer token when logged in. */
export function createApiClient({ baseURL, getToken }: ApiClientOptions): AxiosInstance {
  const api = axios.create({
    baseURL,
    headers: { Accept: "application/json" },
  })

  api.interceptors.request.use((config) => {
    const token = getToken?.()
    if (token) config.headers.set("Authorization", `Bearer ${token}`)
    return config
  })

  return api
}
