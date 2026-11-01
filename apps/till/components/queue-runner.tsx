"use client"

import * as React from "react"
import Link from "next/link"
import { useTranslations } from "next-intl"

import { Badge } from "@genesy/ui/components/badge"
import { pendingCount, replay, useQueue } from "@/lib/sale-queue"

/** Replays queued sales when the till comes back online (and every 30 s); shows how many wait. */
export function QueueRunner() {
  const t = useTranslations("queue")
  const waiting = useQueue(pendingCount)
  const open = useQueue((s) => s.items.length)

  React.useEffect(() => {
    void replay()
    const run = () => void replay()
    window.addEventListener("online", run)
    const timer = window.setInterval(run, 30_000)
    return () => {
      window.removeEventListener("online", run)
      window.clearInterval(timer)
    }
  }, [])

  if (!open) return null
  return (
    // No prefetch: the badge shows up exactly when the network may be gone.
    <Link href="/queue" prefetch={false} className="text-sm">
      <Badge variant={waiting ? "destructive" : "secondary"}>
        {t("badge", { count: waiting })}
      </Badge>
    </Link>
  )
}
