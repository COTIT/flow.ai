import { redirect } from "next/navigation"

/** `/accounting` opens its first section. */
export default function AccountingPage() {
  redirect("/accounting/journals")
}
