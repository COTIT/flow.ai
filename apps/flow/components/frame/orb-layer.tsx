"use client"

import { useCallback, useLayoutEffect, useRef } from "react"
import { usePathname, useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { useVoice } from "@genesy/voice/components/provider"
import { flowStore, useFlow } from "@/lib/flow-store"
import {
  orbMode,
  slotTransform,
  springEasing,
  type OrbMode,
} from "@/lib/orb-geometry"
import { OrbVisual } from "./orb-adapter"

/** The orb is drawn at this size and scaled down into every slot (transform only). */
const BASE = 256
const MOVE_MS = 480
const EASE = springEasing()

/** A reserved place for the orb; the orb layer measures it and moves there. */
export function OrbSlot({
  name,
  className,
}: {
  name: OrbMode
  className: string
}) {
  return (
    <div
      aria-hidden
      data-orb-slot={name}
      className={`shrink-0 ${className}`}
      ref={useCallback(
        (element: HTMLDivElement) => {
          flowStore.getState().setSlot(name, element)
          return () => flowStore.getState().setSlot(name, null)
        },
        [name]
      )}
    />
  )
}

/**
 * ONE orb for the whole signed-in app, inside the viewport region (so never over header or dock).
 * It never remounts on navigation; it glides between slots with a critically damped curve.
 */
export function OrbLayer() {
  const t = useTranslations("shell")
  const router = useRouter()
  const pathname = usePathname()
  const voice = useVoice()
  const immersive = useFlow((s) => s.immersive)
  const resultOpen = useFlow((s) => s.resultOpen)
  const conversing = voice.messages.length > 0
  const sheetRaised = useFlow((s) => s.sheetRaised)
  const mode = orbMode({
    pathname,
    resultOpen,
    immersive,
    conversing,
    sheetRaised,
  })
  // In the conversation itself the orb is only a picture of it (the controls sit beside it).
  const picture =
    pathname === "/" &&
    immersive &&
    !resultOpen &&
    (voice.connected || voice.starting || voice.upgrading)
  const slot = useFlow((s) => s.slots[mode])
  const layer = useRef<HTMLDivElement>(null)
  const orb = useRef<HTMLButtonElement>(null)
  const placed = useRef<OrbMode | null>(null)
  /** Places the orb now (set while a slot is measured); focus uses it to stay in view. */
  const placeNow = useRef<() => void>(() => {})
  const live = voice.connected || voice.starting || voice.upgrading

  useLayoutEffect(() => {
    const element = orb.current
    const box = layer.current
    if (!element || !box) return
    if (!slot) {
      element.style.visibility = "hidden"
      return
    }
    const still = matchMedia("(prefers-reduced-motion: reduce)").matches
    let moving = placed.current !== null && placed.current !== mode && !still
    placed.current = mode
    let frame = 0
    // A late callback (fonts, frame) after cleanup must not measure a detached slot.
    let dead = false
    const settle = setTimeout(() => (moving = false), MOVE_MS + 40)
    const place = () => {
      if (dead) return
      const to = slotTransform(
        slot.getBoundingClientRect(),
        box.getBoundingClientRect(),
        BASE
      )
      element.style.transition = moving
        ? `transform ${MOVE_MS}ms ${EASE}`
        : "none"
      element.style.transform = `translate(${to.x}px, ${to.y}px) scale(${to.scale})`
      element.style.visibility = "visible"
    }
    place()
    placeNow.current = place
    const later = () => {
      if (dead) return
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(place)
    }
    const resize = new ResizeObserver(later)
    resize.observe(box)
    resize.observe(slot)
    const scroller = slot.closest("[data-flow-scroll]")
    scroller?.addEventListener("scroll", later, { passive: true })
    // A slot can move without resizing (the phone sheet rising, the rail opening): follow any
    // pane that changes size, and any transition that ends.
    const host = box.parentElement
    host?.addEventListener("transitionend", later)
    for (const pane of Array.from(host?.children ?? []))
      if (pane !== box) resize.observe(pane)
    window.visualViewport?.addEventListener("resize", later)
    void document.fonts?.ready.then(later)
    return () => {
      dead = true
      clearTimeout(settle)
      cancelAnimationFrame(frame)
      resize.disconnect()
      scroller?.removeEventListener("scroll", later)
      host?.removeEventListener("transitionend", later)
      window.visualViewport?.removeEventListener("resize", later)
    }
  }, [slot, mode])

  const label = mode === "ambient" && !live ? t("orbStart") : t("orbOpen")
  return (
    <div
      ref={layer}
      // Above the panes' sticky header rows (z-10), below the dock layer and the header.
      className="pointer-events-none absolute inset-0 z-20 overflow-clip"
    >
      <button
        ref={orb}
        type="button"
        data-orb={mode}
        aria-label={label}
        // In the conversation the orb is only a picture; elsewhere it opens or starts it.
        tabIndex={picture ? -1 : 0}
        aria-hidden={picture || undefined}
        style={{ width: BASE, height: BASE, visibility: "hidden" }}
        className="pointer-events-auto absolute top-0 left-0 origin-top-left rounded-full focus-visible:ring-4 focus-visible:ring-(--gold) focus-visible:outline-none"
        // A keyboard user never lands on an orb scrolled out of view (SW1): bring its place in.
        onFocus={() => {
          const at = slot?.getBoundingClientRect()
          const view = slot
            ?.closest("[data-flow-scroll]")
            ?.getBoundingClientRect()
          if (!slot || !at || !view) return
          if (at.top < view.top || at.bottom > view.bottom) {
            slot.scrollIntoView({ block: "nearest" })
            placeNow.current()
          }
        }}
        onClick={() => {
          if (picture) return
          if (mode === "ambient" && !live) {
            void voice.start(false)
            return
          }
          flowStore.setState({ immersive: true, resultOpen: false })
          if (pathname !== "/") router.push("/")
        }}
      >
        <OrbVisual size={BASE} />
      </button>
    </div>
  )
}
