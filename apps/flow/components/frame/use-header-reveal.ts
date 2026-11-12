"use client"

import { useEffect, useReducer, useRef, type RefObject } from "react"

import {
  HIDDEN,
  IDLE_MS,
  LEAVE_MS,
  isRevealSwipe,
  reveal,
  type HeaderEvent,
} from "@/lib/header-reveal"

/** Mouse events this soon after a touch are the browser's compat copies of it. */
const GHOST_MS = 800
/** Pointer within this many px of the top edge reveals the header (desktop). */
const EDGE_PX = 8
const POPUP = '[role="listbox"],[role="menu"],[data-slot="select-content"]'
const DIALOG = '[aria-modal="true"],[role="dialog"],[role="alertdialog"]'

const overlaps = (a: DOMRect, b: DOMRect) =>
  a.bottom > b.top && a.top < b.bottom && a.right > b.left && a.left < b.right

/** Wires the pure `reveal` rules to the page: pointer, focus, touch, Escape and timers. */
export function useHeaderReveal(
  header: RefObject<HTMLElement | null>,
  /** A new address hides it (a header link was used); focus left inside moves to the content. */
  path: string
) {
  const [state, dispatch] = useReducer(reveal, HIDDEN)
  const shown = useRef(state.shown)
  useEffect(() => {
    shown.current = state.shown
  }, [state.shown])
  /** The pointer is over the header or its top edge (a later move away is a leave). */
  const pointer = useRef(false)
  /** When a finger last touched: mouse events right after are the browser's compat copies. */
  const touchedAt = useRef(-Infinity)
  const ghost = () => performance.now() - touchedAt.current < GHOST_MS
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const later = (event: HeaderEvent, ms: number) => {
    clearTimeout(timer.current)
    timer.current = setTimeout(() => dispatch(event), ms)
  }

  useEffect(() => {
    const inHeader = (n: EventTarget | null) =>
      n instanceof Node && !!header.current?.contains(n)
    const el = (n: EventTarget | null) => (n instanceof Element ? n : null)

    const move = (e: PointerEvent) => {
      if (e.pointerType !== "mouse" || ghost()) return
      if (e.clientY <= EDGE_PX) {
        pointer.current = true
        return dispatch({ type: "edge" })
      }
      // Revealed from the edge and moved away without ever entering it: that is a leave too.
      if (pointer.current && !inHeader(e.target)) {
        pointer.current = false
        dispatch({ type: "leave" })
        later({ type: "timeout" }, LEAVE_MS)
      }
    }
    const focusin = (e: FocusEvent) => {
      const target = el(e.target)
      // A dialog opened from the header (sign out) sits above it: the header stays, so focus
      // returns to a control on screen when it closes.
      if (target?.closest(DIALOG)) {
        if (!inHeader(e.relatedTarget)) dispatch({ type: "dialog", open: true })
        return
      }
      dispatch({ type: "dialog", open: false })
      if (inHeader(target)) {
        dispatch({ type: "popup", open: false })
        // Focus inside always shows it (a focused control is never off screen); only keyboard
        // focus holds it open, a mouse click inside leaves it to the pointer.
        dispatch({
          type: target?.matches(":focus-visible") ? "focusin" : "swipe",
        })
        return
      }
      if (target?.closest(POPUP)) return dispatch({ type: "popup", open: true })
      dispatch({ type: "popup", open: false })
      const box = header.current?.getBoundingClientRect()
      if (
        shown.current &&
        box &&
        target &&
        overlaps(target.getBoundingClientRect(), box)
      )
        return dispatch({ type: "covers" })
      dispatch({ type: "focusout" })
    }
    const key = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return
      const inside = inHeader(document.activeElement)
      // Focus inside counts even if it never revealed it: Escape never leaves focus on a hidden header.
      if (!shown.current && !inside) return
      // An open menu or dialog closes first (closed popups stay mounted, marked data-closed).
      const open = [...document.querySelectorAll(`${POPUP},${DIALOG}`)].some(
        (el) => !el.closest("[data-closed]")
      )
      if (open) return
      if (inside)
        document.querySelector<HTMLElement>("[data-flow-content]")?.focus()
      dispatch({ type: "escape" })
    }

    // Touch, option A: swipe down from just below the safe area; swipe up on it hides.
    let start: { x: number; y: number; onHeader: boolean } | null = null
    const touchstart = (e: TouchEvent) => {
      touchedAt.current = performance.now()
      const t = e.touches[0]
      if (!t || e.touches.length > 1) return (start = null)
      start = { x: t.clientX, y: t.clientY, onHeader: inHeader(e.target) }
      // The handle toggles on its own click; its touch is not "outside" (P10-5).
      if (
        shown.current &&
        !start.onHeader &&
        !el(e.target)?.closest(`${POPUP},[data-flow-handle]`)
      )
        dispatch({ type: "outside" })
    }
    const touchend = (e: TouchEvent) => {
      touchedAt.current = performance.now()
      const t = e.changedTouches[0]
      if (!start || !t) return
      const dy = t.clientY - start.y
      const dx = t.clientX - start.x
      const safeTop =
        parseFloat(
          getComputedStyle(document.documentElement).getPropertyValue(
            "--flow-safe-top"
          )
        ) || 0
      if (
        isRevealSwipe({
          startY: start.y,
          dy,
          dx,
          safeTop,
        })
      ) {
        dispatch({ type: "swipe" })
        later({ type: "idle" }, IDLE_MS)
      } else if (start.onHeader && dy <= -32) dispatch({ type: "swipeUp" })
      start = null
    }
    const mousedown = (e: MouseEvent) => {
      if (
        shown.current &&
        !inHeader(e.target) &&
        !el(e.target)?.closest(`${POPUP},[data-flow-handle]`)
      )
        dispatch({ type: "outside" })
    }

    document.addEventListener("pointermove", move, { passive: true })
    document.addEventListener("focusin", focusin)
    document.addEventListener("keydown", key)
    document.addEventListener("touchstart", touchstart, { passive: true })
    document.addEventListener("touchend", touchend, { passive: true })
    document.addEventListener("mousedown", mousedown)
    return () => {
      clearTimeout(timer.current)
      document.removeEventListener("pointermove", move)
      document.removeEventListener("focusin", focusin)
      document.removeEventListener("keydown", key)
      document.removeEventListener("touchstart", touchstart)
      document.removeEventListener("touchend", touchend)
      document.removeEventListener("mousedown", mousedown)
    }
  }, [header])

  useEffect(() => {
    if (header.current?.contains(document.activeElement))
      document.querySelector<HTMLElement>("[data-flow-content]")?.focus()
    dispatch({ type: "escape" })
  }, [path, header])

  return {
    shown: state.shown,
    // A finger's compat mouse events never count as hovering (F1): the touch idle timer stays
    // in charge, so the header still hides by itself after a swipe.
    enter: () => {
      if (ghost()) return
      pointer.current = true
      clearTimeout(timer.current)
      dispatch({ type: "enter" })
    },
    leave: () => {
      if (ghost()) return
      pointer.current = false
      dispatch({ type: "leave" })
      later({ type: "timeout" }, LEAVE_MS)
    },
    toggle: () => {
      dispatch({ type: "handle" })
      later({ type: "idle" }, IDLE_MS)
    },
  }
}
