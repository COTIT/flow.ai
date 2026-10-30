"use client"

import * as React from "react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"

/** Gives every screen in an app one shared TanStack Query cache. */
export function QueryProvider({ children }: { children: React.ReactNode }) {
  // One client per browser tab; useState keeps it across re-renders.
  const [client] = React.useState(() => new QueryClient())
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>
}
