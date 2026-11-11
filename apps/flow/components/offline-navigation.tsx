"use client"

import { useEffect, useState } from "react"
import { useTranslations } from "next-intl"

import { Notice } from "@genesy/ui/components/notice"

/** The part of the Navigation API this guard reads (not in TypeScript's DOM types yet). */
type NavigateEvent = Event & {
  cancelable: boolean
  hashChange: boolean
  downloadRequest: string | null
  navigationType: "push" | "replace" | "reload" | "traverse"
  destination: { url: string; sameDocument: boolean }
}
type Navigation = EventTarget

/**
 * Offline, a page the router has not loaded makes Next fall back to a full page load, which
 * lands on the browser's "no internet" page and loses the conversation and the draft (P6-6).
 * While offline this stops in-app links and cancels that load, keeps Flow on screen and says
 * why. Online it does nothing.
 * shortcut: trusts navigator.onLine, so "lie-fi" (connected, no internet) still falls through;
 * upgrade with a reachability check if field reports show it.
 */
export function OfflineNavigation() {
  const t = useTranslations("shell")
  const [blocked, setBlocked] = useState(false)

  useEffect(() => {
    const inFlow = (url: string) =>
      new URL(url, location.href).origin === location.origin
    const online = () => setBlocked(false)
    // Next assigns the URL while rendering a navigation that then suspends forever; a state
    // update made inside that render would wait with it, so it is made after the render.
    const block = () => setTimeout(() => setBlocked(true))
    const navigation = (window as { navigation?: Navigation }).navigation

    const onNavigate = (event: Event) => {
      const e = event as NavigateEvent
      if (navigator.onLine || !e.cancelable || e.hashChange) return
      if (e.downloadRequest !== null || e.destination.sameDocument) return
      if (e.navigationType !== "push" && e.navigationType !== "replace") return
      if (!inFlow(e.destination.url)) return
      e.preventDefault()
      block()
    }
    // Links are stopped before Next starts: a cancelled fallback leaves the router holding that
    // URL, so the same link tapped again once online would do nothing.
    const onClick = (e: MouseEvent) => {
      if (navigator.onLine || e.defaultPrevented || e.button !== 0) return
      if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return
      const a = (e.target as Element | null)?.closest?.("a[href]")
      if (
        !(a instanceof HTMLAnchorElement) ||
        a.target ||
        a.hasAttribute("download")
      )
        return
      if (!inFlow(a.href) || (a.hash && a.pathname === location.pathname))
        return
      e.preventDefault()
      block()
    }

    // shortcut: without the Navigation API (older Safari/Firefox) router.push (Back to home,
    // the orb) is not guarded, only links; drop the check once those browsers age out.
    window.addEventListener("online", online)
    navigation?.addEventListener("navigate", onNavigate)
    document.addEventListener("click", onClick, true)
    return () => {
      window.removeEventListener("online", online)
      navigation?.removeEventListener("navigate", onNavigate)
      document.removeEventListener("click", onClick, true)
    }
  }, [])

  if (!blocked) return null
  return (
    <div className="pointer-events-none fixed inset-x-0 top-[max(1rem,env(safe-area-inset-top))] z-50 flex justify-center px-4">
      <Notice
        variant="info"
        title={t("offlineNavigation")}
        className="pointer-events-auto w-auto max-w-md shadow-lg"
      />
    </div>
  )
}
