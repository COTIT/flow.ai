"use client"

import { useEffect, useState } from "react"
import { useRouter } from "next/navigation"
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
 * why. Online it does nothing. Back online, the page asked for opens by itself once (J3), unless
 * the person did something else meanwhile (a key, tap or typing) or is no longer where they asked.
 * shortcut: trusts navigator.onLine, so "lie-fi" (connected, no internet) still falls through;
 * upgrade with a reachability check if field reports show it.
 */
export function OfflineNavigation() {
  const t = useTranslations("shell")
  const router = useRouter()
  const [blocked, setBlocked] = useState(false)

  useEffect(() => {
    const inFlow = (url: string) =>
      new URL(url, location.href).origin === location.origin
    const here = () => location.pathname + location.search
    // The latest page asked for while offline, and where the person was when they asked.
    let pending: { url: string; from: string } | null = null
    const online = () => {
      setBlocked(false)
      const wanted = pending
      pending = null
      if (wanted && here() === wanted.from) router.push(wanted.url)
    }
    // Any later key, tap or typed input (paste, dictation) is a new choice: the old request no
    // longer opens by surprise.
    const moveOn = () => (pending = null)
    // Next assigns the URL while rendering a navigation that then suspends forever; a state
    // update made inside that render would wait with it, so it is made after the render.
    const block = (url: string) => {
      const target = new URL(url, location.href)
      pending = {
        url: target.pathname + target.search + target.hash,
        from: here(),
      }
      setTimeout(() => setBlocked(true))
    }
    const navigation = (window as { navigation?: Navigation }).navigation

    const onNavigate = (event: Event) => {
      const e = event as NavigateEvent
      if (navigator.onLine || !e.cancelable || e.hashChange) return
      if (e.downloadRequest !== null || e.destination.sameDocument) return
      if (e.navigationType !== "push" && e.navigationType !== "replace") return
      if (!inFlow(e.destination.url)) return
      e.preventDefault()
      block(e.destination.url)
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
      block(a.href)
    }

    // shortcut: without the Navigation API (older Safari/Firefox) router.push (Back to home,
    // the orb) is not guarded, only links; drop the check once those browsers age out.
    window.addEventListener("online", online)
    navigation?.addEventListener("navigate", onNavigate)
    document.addEventListener("click", onClick, true)
    document.addEventListener("keydown", moveOn, true)
    document.addEventListener("pointerdown", moveOn, true)
    document.addEventListener("input", moveOn, true)
    return () => {
      window.removeEventListener("online", online)
      navigation?.removeEventListener("navigate", onNavigate)
      document.removeEventListener("click", onClick, true)
      document.removeEventListener("keydown", moveOn, true)
      document.removeEventListener("pointerdown", moveOn, true)
      document.removeEventListener("input", moveOn, true)
    }
  }, [router])

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
