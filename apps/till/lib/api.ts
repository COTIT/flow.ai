import { configureApi } from "@genesy/api-client/mutator"

const baseURL = process.env.NEXT_PUBLIC_API_URL
if (!baseURL) throw new Error("NEXT_PUBLIC_API_URL is not set (see .env.example)")

/** This app's connection to the Genesy backend; generated hooks send through it too. */
export const api = configureApi({ baseURL })
