"use client"

import * as React from "react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"

import { sessionStore } from "./session"
import { connectBrowser } from "./setup"

if (typeof window !== "undefined") connectBrowser()

/** Gives every screen in an app one shared TanStack Query cache, the backend connection and the saved login. */
export function QueryProvider({ children }: { children: React.ReactNode }) {
  // One client per browser tab; useState keeps it across re-renders.
  const [client] = React.useState(() => new QueryClient())
  // Read the saved login after the first render, so server and browser HTML match.
  React.useEffect(() => {
    void sessionStore.persist.rehydrate()
  }, [])
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>
}
