"use client"

import { useEffect, useRef } from "react"

/**
 * Moves focus to the returned ref's element whenever `when` changes (e.g. a new review ticket),
 * so a keyboard user follows the task when the button they pressed is replaced. Never takes focus
 * from someone typing (the composer or any field).
 */
export function useTakeFocus<T extends HTMLElement>(when: unknown) {
  const ref = useRef<T>(null)
  useEffect(() => {
    const active = document.activeElement as HTMLElement | null
    const typing = active?.matches(
      "input, textarea, select, [contenteditable='true']"
    )
    if (!when || typing || !ref.current) return
    ref.current.focus({ preventScroll: true })
    // In view inside its own pane (a short screen may have it below the fold, R2).
    ref.current.scrollIntoView({ block: "nearest" })
  }, [when])
  return ref
}
