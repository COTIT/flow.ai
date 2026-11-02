"use client"

import { useEffect } from "react"
import { useRouter } from "next/navigation"

/**
 * Re-reads the order page every 3 s while it awaits payment: the provider's webhook can land
 * after the buyer is back. Mounted only while unpaid, so a status change stops it; it skips
 * hidden tabs and gives up after 2 minutes. The API decides the status; this only re-reads it.
 */
export function RefreshWhileUnpaid() {
  const router = useRouter()
  useEffect(() => {
    const until = Date.now() + 2 * 60_000
    const timer = setInterval(() => {
      if (Date.now() > until) clearInterval(timer)
      else if (document.visibilityState === "visible") router.refresh()
    }, 3000)
    return () => clearInterval(timer)
  }, [router])
  return null
}
