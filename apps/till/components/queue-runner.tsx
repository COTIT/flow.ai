"use client"

import * as React from "react"
import Link from "next/link"
import { useTranslations } from "next-intl"

import { pendingCount, replay, useQueue } from "@/lib/sale-queue"
import { useOnline } from "@/lib/use-online"

/**
 * Replays queued sales when the till comes back online (and every 30 s). Its badge says
 * Online / Offline and how many wait, linking to them.
 */
export function QueueRunner() {
  const t = useTranslations("queue")
  const waiting = useQueue(pendingCount)
  const open = useQueue((s) => s.items.length)
  const online = useOnline()

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

  const badge = (
    <span className={`tbadge ${online && !waiting ? "teal" : "gold"}`}>
      {online
        ? open
          ? t("badge", { count: waiting })
          : t("online")
        : t("offline", { count: waiting })}
    </span>
  )
  if (!open) return online ? <span className="no-phone">{badge}</span> : badge
  return (
    // No prefetch: the badge shows up exactly when the network may be gone.
    <Link href="/queue" prefetch={false} className="badge-link">
      {badge}
    </Link>
  )
}
