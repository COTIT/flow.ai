"use client"

import * as React from "react"
import { onlineManager, useQueryClient } from "@tanstack/react-query"

import { configureApi } from "@genesy/api-client/mutator"
import { catalogStore } from "@/lib/use-catalog"
import { queueStore } from "@/lib/sale-queue"
import { tillStore } from "@/lib/till-store"

/**
 * Points the API client at the till session (not the member login QueryProvider sets up),
 * reads the saved pairing and the real network state, and empties the query cache when a till user logs out.
 */
export function TillProvider({ children }: { children: React.ReactNode }) {
  const queryClient = useQueryClient()
  // During the first render, so before any child query runs; QueryProvider's default ran at import.
  React.useState(() =>
    configureApi({
      baseURL: "/v1",
      getToken: () => tillStore.getState().session?.token ?? null,
      // A 401 ends the till session; the device stays paired (a bad device key is handled on /login).
      onUnauthorized: () => tillStore.getState().signOut("expired"),
    })
  )
  React.useEffect(() => {
    // TanStack assumes online until an online/offline event; a till opened with no network gets none.
    onlineManager.setOnline(navigator.onLine)
    void tillStore.persist.rehydrate()
    void queueStore.persist.rehydrate()
    void catalogStore.persist.rehydrate()
  }, [])
  React.useEffect(
    () =>
      tillStore.subscribe((now, before) => {
        if (before.session && !now.session) queryClient.clear()
      }),
    [queryClient]
  )
  return children
}
