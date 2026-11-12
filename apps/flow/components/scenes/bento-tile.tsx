"use client"

import { useLayoutEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { ChevronDown, ChevronUp } from "lucide-react"

import { Button } from "@genesy/ui/components/button"
import { TRACK_PX } from "@/lib/bento"

const GAP_PX = 16
/** The footer strip that closes a cut card ("Show all"). */
const FOOTER_PX = 56

const tracksFor = (px: number) => Math.ceil((px + GAP_PX) / (TRACK_PX + GAP_PX))

/**
 * One bento tile on the fixed row tracks. A tile whose content fits the space left on screen
 * grows to show all of it; otherwise it is cut at its edge, closed by a footer strip in the card's
 * own frame with "Show all", which grows this tile only. The scene keeps one scroller.
 */
export function BentoTile({
  id,
  size,
  className,
  children,
}: {
  id: string
  size: string
  className: string
  children: React.ReactNode
}) {
  const t = useTranslations("scenes")
  const tile = useRef<HTMLDivElement>(null)
  const [over, setOver] = useState(false)
  /** Grown to fit by itself (room on screen) or by "Show all". */
  const [rows, setRows] = useState<{ n: number; asked: boolean } | null>(null)
  useLayoutEffect(() => {
    const el = tile.current
    if (!el) return
    const measure = () => {
      const fixed = getComputedStyle(el).overflowY === "hidden"
      const cut = fixed && el.scrollHeight > el.clientHeight + 1
      if (cut && !rows?.asked) {
        // Room left on screen below: grow by whole tracks, up to all of the content.
        const room = el.closest("[data-flow-scroll]")?.getBoundingClientRect()
        const top = el.getBoundingClientRect().top
        const now = tracksFor(el.clientHeight)
        const fit = room
          ? Math.floor((room.bottom - top + GAP_PX) / (TRACK_PX + GAP_PX))
          : now
        const n = Math.min(tracksFor(el.scrollHeight), fit)
        if (n > now) {
          setRows({ n, asked: false })
          return
        }
      }
      setOver(cut)
    }
    measure()
    const watch = new ResizeObserver(measure)
    watch.observe(el)
    if (el.firstElementChild) watch.observe(el.firstElementChild)
    return () => watch.disconnect()
  }, [rows])
  const asked = rows?.asked ?? false
  return (
    <div
      ref={tile}
      data-component={id}
      data-size={size}
      // Keyboard focus on a row behind the cut opens the tile: focus is never hidden.
      onFocus={(e) => {
        const el = tile.current
        const target = e.target as Element
        if (!el || !over || rows?.asked || target.closest("[data-tile-footer]"))
          return
        const cut = el.getBoundingClientRect().bottom - FOOTER_PX - 32
        if (target.getBoundingClientRect().bottom > cut)
          setRows({ n: tracksFor(el.scrollHeight + FOOTER_PX), asked: true })
      }}
      style={rows ? { gridRow: `span ${rows.n}` } : undefined}
      className={`relative min-w-0 [&>*:first-child]:h-full ${rows?.asked ? "" : "@min-[640px]:overflow-hidden"} ${className}`}
    >
      {children}
      {over && !rows?.asked ? (
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
              onClick={() => {
                const el = tile.current
                if (el)
                  setRows({
                    n: tracksFor(el.scrollHeight + FOOTER_PX),
                    asked: true,
                  })
              }}
            >
              <ChevronDown aria-hidden />
              {t("showAll")}
            </Button>
          </div>
        </div>
      ) : null}
      {asked ? (
        <div className="flex justify-center pt-2">
          <Button
            variant="outline"
            size="sm"
            className="min-h-11 rounded-full"
            aria-expanded
            onClick={() => setRows(null)}
          >
            <ChevronUp aria-hidden />
            {t("showLess")}
          </Button>
        </div>
      ) : null}
    </div>
  )
}
