import { connection } from "next/server"

import { exactOrigin } from "@/lib/links"
import { PurchaseApprovals } from "./purchases"

/** Read per request: mapping fixes open in Office, whose address differs per environment. */
export default async function PurchaseApprovalsPage() {
  await connection()
  return (
    <PurchaseApprovals
      office={exactOrigin(process.env.OFFICE_PUBLIC_URL) ?? null}
    />
  )
}
