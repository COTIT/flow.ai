"use client"

import { useLayoutEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { ChevronDown, ChevronUp } from "lucide-react"

import { Button } from "@genesy/ui/components/button"

/** The footer strip that closes a cut card ("Show all (n)"). */
const FOOTER_PX = 56
/** Below this the cut would leave too little to read (one row and the footer): natural height. */
const MIN_CAP_PX = 140
/** Rows a list tile shows: table rows, list items. */
const ROWS = "tbody tr, [data-item], li"

/**
 * One bento tile at its content's natural height (bento fit). On a tablet or computer a tile taller
 * than the room left on screen is cut there, closed by a footer with "Show all (n)", which opens
 * this tile only; the scene keeps one scroller. A phone stacks tiles at their natural height.
 */
export function BentoTile({
  id,
  size,
  className,
  style,
  children,
}: {
  id: string
  size: string
  className: string
  style?: React.CSSProperties
  children: React.ReactNode
}) {
  const t = useTranslations("scenes")
  const tile = useRef<HTMLDivElement>(null)
  const [open, setOpen] = useState(false)
  const [cap, setCap] = useState<{ px: number; rows: number } | null>(null)
  useLayoutEffect(() => {
    const el = tile.current
    if (!el || open) return setCap(null)
    const measure = () => {
      if (window.innerWidth < 640) return setCap(null)
      const view = el.closest("[data-flow-scroll]")?.getBoundingClientRect()
      const content =
        (el.firstElementChild as HTMLElement | null)?.scrollHeight ?? 0
      const room = view
        ? Math.floor(view.bottom - el.getBoundingClientRect().top - 8)
        : Infinity
      const cut = content > room && room >= MIN_CAP_PX
      setCap((was) =>
        cut
          ? was?.px === room
            ? was
            : { px: room, rows: el.querySelectorAll(ROWS).length }
          : null
      )
    }
    measure()
    const watch = new ResizeObserver(measure)
    if (el.firstElementChild) watch.observe(el.firstElementChild)
    // A neighbour above growing moves this tile down: measure again.
    if (el.parentElement) watch.observe(el.parentElement)
    return () => watch.disconnect()
  }, [open])
  return (
    <div
      ref={tile}
      data-component={id}
      data-size={size}
      // Keyboard focus on a row behind the cut opens the tile: focus is never hidden.
      onFocus={(e) => {
        const el = tile.current
        if (!el || !cap || (e.target as Element).closest("[data-tile-footer]"))
          return
        if (
          (e.target as Element).getBoundingClientRect().bottom >
          el.getBoundingClientRect().bottom - FOOTER_PX
        ) {
          setOpen(true)
          // Opened by focus behind the cut: bring the focused control into view.
          const target = e.target as HTMLElement
          requestAnimationFrame(() =>
            target.scrollIntoView({ block: "nearest" })
          )
        }
      }}
      style={{ ...style, ...(cap ? { maxHeight: cap.px } : {}) }}
      className={`relative min-w-0 [&>*:first-child]:h-full ${cap ? "overflow-hidden" : ""} ${className}`}
    >
      {children}
      {cap ? (
        <div
          data-tile-footer
          className="absolute inset-x-0 bottom-0 flex flex-col"
        >
          {/* The rows fade out above the footer; the footer closes the card's frame. */}
          <div
            aria-hidden
            className="h-8 bg-gradient-to-t from-card to-transparent"
          />
          <div
            style={{ height: FOOTER_PX }}
            className="flex items-center justify-center rounded-b-2xl border border-t-0 bg-card"
          >
            <Button
              variant="outline"
              size="sm"
              className="min-h-11 rounded-full"
              aria-expanded={false}
              onClick={() => setOpen(true)}
            >
              <ChevronDown aria-hidden />
              {cap.rows ? t("showAllCount", { count: cap.rows }) : t("showAll")}
            </Button>
          </div>
        </div>
      ) : null}
      {open ? (
        <div className="flex justify-center pt-2">
          <Button
            variant="outline"
            size="sm"
            className="min-h-11 rounded-full"
            aria-expanded
            onClick={() => setOpen(false)}
          >
            <ChevronUp aria-hidden />
            {t("showLess")}
          </Button>
        </div>
      ) : null}
    </div>
  )
}
