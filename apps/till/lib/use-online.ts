"use client"

import * as React from "react"
import { onlineManager } from "@tanstack/react-query"

/** Whether the browser says it has a network, kept current by TanStack's online manager. */
export function useOnline() {
  return React.useSyncExternalStore(
    onlineManager.subscribe.bind(onlineManager),
    () => onlineManager.isOnline(),
    () => true
  )
}
