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
  const shown = useRef(false)
  const measured = useRef(-1)
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
    const later = () => {
      cancelAnimationFrame(raf)
      raf = requestAnimationFrame(place)
    }
    function place() {
      if (!box || !frame || !slot) return
      const to = slot.getBoundingClientRect()
      const from = frame.getBoundingClientRect()
      box.style.left = `${to.left - from.left}px`
      // Anchored by its bottom edge (every slot sits at the bottom of its pane): a dock that
      // grows or a row that settles never leaves it hanging past the screen.
      box.style.bottom = `${from.bottom - to.bottom}px`
      box.style.width = `${to.width}px`
      const height = box.offsetHeight
      frame.style.setProperty("--flow-dock-h", `${height}px`)
      // First load: it appears once its height has settled (two equal measures), so the page
      // never shifts under a growing dock (SW8). After that it is never hidden again.
      if (!shown.current) {
        if (height === measured.current) {
          shown.current = true
          box.style.visibility = "visible"
        } else {
          measured.current = height
          later()
        }
      }
    }
    place()
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
