import { createApiClient } from "@genesy/api-client/client"

const baseURL = process.env.NEXT_PUBLIC_API_URL
if (!baseURL) throw new Error("NEXT_PUBLIC_API_URL is not set (see .env.example)")

/** This app's connection to the Genesy backend. */
export const api = createApiClient({ baseURL })
