"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"

/**
 * Shown while an order awaits payment. Re-reads the page every 3 s (the provider's webhook
 * can land after the buyer is back), skipping hidden tabs. After 2 minutes it stops and
 * puts a warning above `children` with "Check again". Mounted only while unpaid, so a
 * status change stops it. The API decides the status; this only re-reads it.
 */
export function PaymentWatcher({ children }: { children: React.ReactNode }) {
  const t = useTranslations("order.waitingLong")
  const router = useRouter()
  const [round, setRound] = React.useState(0)
  const [gaveUp, setGaveUp] = React.useState(false)

  React.useEffect(() => {
    const until = Date.now() + 2 * 60_000
    const timer = setInterval(() => {
      if (Date.now() > until) {
        clearInterval(timer)
        setGaveUp(true)
      } else if (document.visibilityState === "visible") router.refresh()
    }, 3000)
    return () => clearInterval(timer)
  }, [router, round])

  if (!gaveUp) return children
  // The warning comes first; the order's own notice (with its pay button) stays below it.
  return (
    <>
      <Notice
        live={false}
        variant="warning"
        title={t("title")}
        action={
          <Button
            variant="outline"
            onClick={() => {
              setGaveUp(false)
              setRound((r) => r + 1)
              router.refresh()
            }}
          >
            {t("action")}
          </Button>
        }
      >
        {t("body")}
      </Notice>
      {children}
    </>
  )
}
