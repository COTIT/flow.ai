"use client"

import { useEffect, useState } from "react"
import { useSession } from "@genesy/api-client/auth"
import { sessionStore } from "@genesy/api-client/session"
import { useUsersMe } from "@genesy/api-client/endpoints/users"
import { useVoice } from "@genesy/voice/components/provider"
import { readHistoryPointer, saveHistoryPointer } from "@/lib/history-pointer"

export function HistoryContinuity() {
  const voice = useVoice()
  const workspaceId = useSession((s) => s.workspaceId)
  const user = useUsersMe().data?.data.user
  const owner = user?.id && workspaceId ? `${user.id}:${workspaceId}` : null
  return voice.historyEnabled && owner ? (
    <ScopedContinuity key={owner} owner={owner} />
  ) : null
}

function ScopedContinuity({ owner }: { owner: string }) {
  const voice = useVoice()
  const [ready, setReady] = useState(false)
  useEffect(() => {
    let active = true
    const id = readHistoryPointer(sessionStorage, owner)
    // The provider authenticates and reads the server history before hydrating; never opens a mic/socket.
    const restore =
      id && !voice.connected && !voice.starting
        ? voice.restoreConversation(id)
        : Promise.resolve()
    void restore
      .catch(() => saveHistoryPointer(sessionStorage, owner, null))
      .finally(() => {
        if (active) setReady(true)
      })
    return () => {
      active = false
    }
    // One restoration per authenticated owner, not per voice state change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [owner])
  useEffect(() => {
    if (ready)
      saveHistoryPointer(sessionStorage, owner, voice.logicalConversationId)
  }, [owner, ready, voice.logicalConversationId])
  useEffect(
    () =>
      sessionStore.subscribe((now, before) => {
        if (
          now.workspaceId !== before.workspaceId ||
          (before.token && !now.token && !now.expired)
        )
          saveHistoryPointer(sessionStorage, owner, null)
      }),
    [owner]
  )
  return null
}
