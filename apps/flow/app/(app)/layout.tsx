import { connection } from "next/server"

import { SignedIn } from "@/components/signed-in"
import { exactOrigin } from "@/lib/links"

/** Read per request, never at build: one Flow image serves staging and production. */
export default async function AppLayout({
  children,
}: {
  children: React.ReactNode
}) {
  await connection()
  const office = exactOrigin(process.env.OFFICE_PUBLIC_URL) ?? null
  return <SignedIn office={office}>{children}</SignedIn>
}
