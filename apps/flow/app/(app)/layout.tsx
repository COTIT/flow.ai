"use client"

import * as React from "react"
import { useRouter } from "next/navigation"

import { useSession } from "@genesy/api-client/auth"
import { Shell } from "@/components/shell"

/** Signed-in frame: send a logged-out visitor to /login, then the assistant-first shell. */
export default function AppLayout({ children }: { children: React.ReactNode }) {
  const router = useRouter()
  const hydrated = useSession((s) => s.hydrated)
  const token = useSession((s) => s.token)

  React.useEffect(() => {
    if (hydrated && !token) router.replace("/login")
  }, [hydrated, token, router])

  if (!hydrated || !token) return null
  return <Shell>{children}</Shell>
}
