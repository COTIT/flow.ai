"use client"

import * as React from "react"

/** Registers the app's /sw.js (offline start, home-screen app). Production builds only: dev hot reload stays uncached. */
export function ServiceWorker() {
  React.useEffect(() => {
    if (process.env.NODE_ENV !== "production") return
    if (!("serviceWorker" in navigator)) return
    void navigator.serviceWorker.register("/sw.js")
  }, [])
  return null
}
