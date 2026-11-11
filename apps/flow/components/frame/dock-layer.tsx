"use client"

import {
  useCallback,
  useLayoutEffect,
  useRef,
  useSyncExternalStore,
} from "react"

import type { SessionPhase } from "@genesy/voice/lib/presentation"
import { Dock } from "./dock"

/**
 * ONE dock for the whole signed-in app. It is drawn once, in a layer over the frame, and placed
 * over whichever slot the layout shows (the bottom row, the rail, the phone sheet). It never
 * remounts or moves in the DOM, so focus, the draft, an IME composition and a pending send all
 * survive every layout change. The slot reserves the dock's measured height.
 */
let current: HTMLElement | null = null
const listeners = new Set<() => void>()
const subscribe = (l: () => void) => {
  listeners.add(l)
  return () => listeners.delete(l)
}
const setCurrent = (el: HTMLElement | null) => {
  current = el
  listeners.forEach((l) => l())
}

/** Where the dock shows; only one is mounted at a time. */
export function DockSlot({ className = "" }: { className?: string }) {
  return (
    <div
      data-dock-slot
      aria-hidden
      style={{ height: "var(--flow-dock-h)" }}
      className={`w-full min-w-0 shrink-0 ${className}`}
      ref={useCallback((el: HTMLDivElement) => {
        setCurrent(el)
        return () => {
          if (current === el) setCurrent(null)
        }
      }, [])}
    />
  )
}

export function DockLayer({ phase }: { phase: SessionPhase["phase"] }) {
  const layer = useRef<HTMLDivElement>(null)
  const slot = useSyncExternalStore(
    subscribe,
    () => current,
    () => null
  )
  useLayoutEffect(() => {
    const box = layer.current
    const frame = box?.offsetParent as HTMLElement | null
    // Between two slots (one unmounts, the next mounts) it keeps its last place: hiding it,
    // even for a frame, would drop the person's focus.
    if (!box || !frame || !slot) return
    let raf = 0
    const place = () => {
      const to = slot.getBoundingClientRect()
      const from = frame.getBoundingClientRect()
      box.style.left = `${to.left - from.left}px`
      box.style.top = `${to.top - from.top}px`
      box.style.width = `${to.width}px`
      box.style.visibility = "visible"
      frame.style.setProperty("--flow-dock-h", `${box.offsetHeight}px`)
    }
    place()
    const later = () => {
      cancelAnimationFrame(raf)
      raf = requestAnimationFrame(place)
    }
    const watch = new ResizeObserver(later)
    watch.observe(slot)
    watch.observe(frame)
    watch.observe(box)
    window.visualViewport?.addEventListener("resize", later)
    return () => {
      cancelAnimationFrame(raf)
      watch.disconnect()
      window.visualViewport?.removeEventListener("resize", later)
    }
  }, [slot])
  return (
    <div
      ref={layer}
      data-dock-layer
      style={{ visibility: "hidden" }}
      className="absolute z-[36] min-w-0"
    >
      <Dock phase={phase} />
    </div>
  )
}
