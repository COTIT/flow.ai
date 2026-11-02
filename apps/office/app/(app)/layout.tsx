"use client"

import * as React from "react"
import { useRouter } from "next/navigation"

import { useSession } from "@genesy/api-client/auth"
import { useUsersWorkspaces } from "@genesy/api-client/endpoints/users"
import { AppShell } from "@/components/shell"
import { LoadError } from "@/components/states"

/** Signed-in frame for every back-office screen: guard, then the glass shell. */
export default function AppLayout({ children }: { children: React.ReactNode }) {
  const router = useRouter()
  const hydrated = useSession((s) => s.hydrated)
  const token = useSession((s) => s.token)
  const workspaces = useUsersWorkspaces({ query: { enabled: Boolean(token) } })

  React.useEffect(() => {
    if (hydrated && !token) router.replace("/login")
  }, [hydrated, token, router])

  if (!hydrated || !token) return null

  return (
    <AppShell>
      {workspaces.isError ? (
        <LoadError
          error={workspaces.error}
          onRetry={() => void workspaces.refetch()}
        />
      ) : null}
      {children}
    </AppShell>
  )
}
