"use client"

import { useLayoutEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { ChevronDown, ChevronUp } from "lucide-react"

import { Button } from "@genesy/ui/components/button"
import { TRACK_PX } from "@/lib/bento"

const GAP_PX = 16

/**
 * One bento tile on the fixed row tracks. Content taller than the tile (a long table) is cut at
 * the tile's edge with "Show all", which grows this tile only (never the row beside it); the
 * scene keeps one scroller.
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
  const [rows, setRows] = useState<number | null>(null)
  useLayoutEffect(() => {
    const el = tile.current
    if (!el) return
    const measure = () => {
      const fixed = getComputedStyle(el).overflowY === "hidden"
      setOver(fixed && el.scrollHeight > el.clientHeight + 1)
    }
    measure()
    const watch = new ResizeObserver(measure)
    watch.observe(el)
    if (el.firstElementChild) watch.observe(el.firstElementChild)
    return () => watch.disconnect()
  }, [rows])
  const open = rows !== null
  return (
    <div
      ref={tile}
      data-component={id}
      data-size={size}
      style={open ? { gridRow: `span ${rows}` } : undefined}
      className={`relative min-w-0 [&>*:first-child]:h-full ${open ? "" : "@min-[640px]:overflow-hidden"} ${className}`}
    >
      {children}
      {over || open ? (
        <div
          className={`flex justify-center ${open ? "pt-2" : "absolute inset-x-px bottom-px rounded-b-2xl bg-gradient-to-t from-card via-card/90 to-transparent pt-8 pb-2"}`}
        >
          <Button
            variant="outline"
            size="sm"
            className="min-h-11 rounded-full"
            aria-expanded={open}
            onClick={() => {
              const el = tile.current
              if (open || !el) return setRows(null)
              // Tracks the whole content needs (track + gap each), so the tile grows to fit.
              const need = el.scrollHeight + 56
              setRows(Math.ceil((need + GAP_PX) / (TRACK_PX + GAP_PX)))
            }}
          >
            {open ? <ChevronUp aria-hidden /> : <ChevronDown aria-hidden />}
            {t(open ? "showLess" : "showAll")}
          </Button>
        </div>
      ) : null}
    </div>
  )
}
