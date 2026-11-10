"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import { GenesyVoiceProvider } from "@genesy/voice/components/provider"
import { Shell } from "@/components/shell"
import { flowHref } from "@/lib/links"
import { followOtherTabs } from "@/lib/other-tabs"

/**
 * Signed-in frame: send a logged-out visitor to /login, then the assistant around the shell.
 * The voice provider lives here, inside (app) only, so routes outside it never reach /api/voice.
 * Its defaults (/api/voice, /voice-audio) are Flow's own proxy and audio files.
 */
export function SignedIn({
  office,
  children,
}: {
  office: string | null
  children: React.ReactNode
}) {
  const router = useRouter()
  const client = useQueryClient()
  const hydrated = useSession((s) => s.hydrated)
  const token = useSession((s) => s.token)
  const resolveHref = React.useCallback(
    (href: string) => flowHref(href, office),
    [office]
  )

  // A logout, login or workspace switch in another tab reaches this one too.
  React.useEffect(() => followOtherTabs(client), [client])

  React.useEffect(() => {
    if (hydrated && !token) router.replace("/login")
  }, [hydrated, token, router])

  if (!hydrated || !token) return null
  return (
    <GenesyVoiceProvider resolveHref={resolveHref}>
      <Shell>{children}</Shell>
    </GenesyVoiceProvider>
  )
}
