import { connection } from "next/server"

import { exactOrigin } from "@/lib/links"
import { Studio } from "./studio"

/** Read per request, never at build: one Flow image serves staging and production. */
export default async function StorefrontPage() {
  await connection()
  return <Studio shop={exactOrigin(process.env.SHOP_PUBLIC_URL) ?? null} />
}
