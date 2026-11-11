"use client"

import { useEffect, useSyncExternalStore } from "react"
import Link from "next/link"
import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { isLoadFailure } from "@/lib/load-failure"

const watchOnline = (changed: () => void) => {
  addEventListener("online", changed)
  addEventListener("offline", changed)
  return () => {
    removeEventListener("online", changed)
    removeEventListener("offline", changed)
  }
}

/**
 * A screen that failed. Its code or data never arrived (P3-5): say so, and load that page again
 * for real once online (reset() would re-render the same failed import). A real crash: reset().
 * Either way, a way home.
 */
export function ScreenError({ error, reset }: { error: Error; reset: () => void }) {
  const t = useTranslations("flow")
  const load = isLoadFailure(error)
  const online = useSyncExternalStore(watchOnline, () => navigator.onLine, () => true)

  // Back online: load the page once by itself.
  useEffect(() => {
    if (!load) return
    const again = () => location.reload()
    addEventListener("online", again, { once: true })
    return () => removeEventListener("online", again)
  }, [load])

  return (
    <Notice
      variant="error"
      title={load ? t("loadFailed") : t("crashed")}
      action={
        <div className="flex flex-wrap items-center gap-3">
          <Button
            variant="outline"
            size="sm"
            onClick={() => (load ? navigator.onLine && location.reload() : reset())}
          >
            {t("retry")}
          </Button>
          <Link className="text-sm font-medium underline-offset-4 hover:underline" href="/">
            {t("goHome")}
          </Link>
        </div>
      }
    >
      {load ? (online ? t("loadFailedText") : t("loadFailedOffline")) : t("crashedText")}
    </Notice>
  )
}
