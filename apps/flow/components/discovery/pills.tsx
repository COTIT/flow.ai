"use client"

import { useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { dockAsk, usePills, type Pill } from "@/lib/pills"

const EXIT_MS = 150

/**
 * "Suggested for you" (or "Here's what you can do next"). Keys are capability ids, so pills that
 * stay keep their place; new ones fade in, removed ones fade out. The row keeps its height, so
 * the composer below never jumps; on a phone it scrolls sideways.
 */
export function Pills({
  ask,
  disabled,
}: {
  /** Sends an instruction through the dock, as typing it and pressing Enter would. */
  ask: (text: string) => void
  /** While an action waits for approval nothing here can cancel it by accident. */
  disabled?: boolean
}) {
  const t = useTranslations("discovery")
  const { next, pills } = usePills(ask)
  useEffect(() => {
    dockAsk.setState({ ask })
    return () => dockAsk.setState({ ask: undefined })
  }, [ask])
  const row = useRef<HTMLUListElement>(null)
  const [leaving, setLeaving] = useState<Pill[]>([])
  const previous = useRef<Pill[]>(pills)

  const ids = pills.map((p) => p.id).join()
  useEffect(() => {
    const gone = previous.current.filter(
      (p) => !pills.some((q) => q.id === p.id)
    )
    previous.current = pills
    // The focused pill leaving: focus goes to the composer, never to the page body.
    const focused = row.current?.contains(document.activeElement)
      ? document.activeElement?.getAttribute("data-pill")
      : null
    if (focused && gone.some((p) => p.id === focused))
      row.current
        ?.closest("section")
        ?.querySelector<HTMLInputElement>("input:not([type=file])")
        ?.focus()
    if (!gone.length) return
    setLeaving(gone)
    const timer = setTimeout(() => setLeaving([]), EXIT_MS)
    return () => clearTimeout(timer)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ids])

  // The row is drawn from the first paint, empty until suggestions arrive, so the dock never
  // grows under the page as they load (SW8).
  const none = !pills.length && !leaving.length
  return (
    <div
      data-pills
      inert={disabled}
      aria-hidden={none || undefined}
      className="flex min-w-0 flex-col gap-1"
    >
      <p
        id="flow-pills-title"
        className="min-h-4 text-xs text-muted-foreground [@media(max-height:500px)]:sr-only"
        aria-live="polite"
      >
        {none ? null : t(next ? "next" : "suggested")}
      </p>
      <ul
        ref={row}
        aria-labelledby="flow-pills-title"
        className="flex h-11 min-w-0 gap-2 overflow-x-auto overflow-y-hidden overscroll-x-contain"
      >
        {pills.map((pill) => (
          <li
            key={pill.id}
            className="shrink-0 animate-in duration-200 fade-in slide-in-from-bottom-1 motion-reduce:animate-none"
          >
            <Button
              variant="outline"
              data-pill={pill.id}
              className="min-h-11 rounded-full"
              onClick={pill.run}
            >
              {pill.label}
            </Button>
          </li>
        ))}
        {leaving.map((pill) => (
          <li
            key={`leaving-${pill.id}`}
            aria-hidden
            className="shrink-0 animate-out duration-150 fill-mode-forwards fade-out motion-reduce:hidden"
          >
            <Button
              variant="outline"
              tabIndex={-1}
              className="min-h-11 rounded-full"
            >
              {pill.label}
            </Button>
          </li>
        ))}
      </ul>
    </div>
  )
}
