import { configureApi } from "@genesy/api-client/mutator"

/**
 * Server-side connection to the Genesy backend (browser calls are set up by QueryProvider).
 * It holds no login: pass `{ headers: { Authorization: `Bearer ${token}` } }` on each call.
 */
export const api = configureApi({
  baseURL: process.env.API_URL ?? "http://localhost:8080/v1",
})
