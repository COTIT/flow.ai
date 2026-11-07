"use client"

import * as React from "react"
import { usePathname, useRouter } from "next/navigation"

import { useSession } from "@genesy/api-client/auth"
import { useUsersWorkspaces } from "@genesy/api-client/endpoints/users"
import { GenesyVoiceProvider } from "@/components/voice/provider"
import { AppShell } from "@/components/shell"
import { LoadError, NoAccess } from "@/components/states"
import { rightFor, useRights } from "@/lib/can"

/** Signed-in frame for every back-office screen: guard, then the glass shell. */
export default function AppLayout({ children }: { children: React.ReactNode }) {
  const router = useRouter()
  const hydrated = useSession((s) => s.hydrated)
  const token = useSession((s) => s.token)
  const workspaces = useUsersWorkspaces({ query: { enabled: Boolean(token) } })
  // A page whose list the member may not read: say so instead of letting it hit the API's 403.
  const right = rightFor(usePathname())
  const rights = useRights()

  React.useEffect(() => {
    if (hydrated && !token) router.replace("/login")
  }, [hydrated, token, router])

  if (!hydrated || !token) return null

  return (
    <GenesyVoiceProvider>
      <AppShell>
        {workspaces.isError ? (
          <LoadError
            error={workspaces.error}
            onRetry={() => void workspaces.refetch()}
          />
        ) : null}
        {!right || rights.may(right) ? (
          children
        ) : rights.read ? (
          <NoAccess />
        ) : null}
      </AppShell>
    </GenesyVoiceProvider>
  )
}
