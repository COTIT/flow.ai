/**
 * The conversation beside the results. Desktop/tablet: a rail on the right (collapsible,
 * resizable 320–520 px); phone: a bottom sheet (peek ↔ open). State is remembered per tab.
 */
export type Band = "phone" | "tablet" | "desktop"

export const band = (width: number): Band =>
  width < 640 ? "phone" : width < 1024 ? "tablet" : "desktop"

export const RAIL = { min: 320, max: 520, step: 20, desktop: 400, tablet: 340 }

export const clampWidth = (w: number) =>
  Math.round(Math.min(RAIL.max, Math.max(RAIL.min, w)))

/** The rail sits on the right, so moving its edge left (ArrowLeft) widens it. */
export function keyWidth(w: number, key: string): number | null {
  if (key === "ArrowLeft") return clampWidth(w + RAIL.step)
  if (key === "ArrowRight") return clampWidth(w - RAIL.step)
  if (key === "Home") return RAIL.min
  if (key === "End") return RAIL.max
  return null
}

/** A choice to hide or show applies where it was made (a scene, or a review). */
export const railContext = (b: Band, review: boolean) =>
  `${b}:${review ? "review" : "scene"}`

export type RailStore = { open: Record<string, boolean>; width: number | null }

export function railOpen({
  band: b,
  review,
  stored,
}: {
  band: Band
  review: boolean
  stored: Record<string, boolean>
}) {
  if (b === "phone") return false
  return stored[railContext(b, review)] ?? (b === "desktop" || review)
}

const CONTEXT = /^(tablet|desktop):(scene|review)$/

/** Reads the per-tab memory; anything unexpected is dropped, never trusted. */
export function readRail(raw: string | null): RailStore {
  const empty: RailStore = { open: {}, width: null }
  if (!raw) return empty
  let value: unknown
  try {
    value = JSON.parse(raw)
  } catch {
    return empty
  }
  if (!value || typeof value !== "object") return empty
  const { open, width } = value as { open?: unknown; width?: unknown }
  const kept: Record<string, boolean> = {}
  if (open && typeof open === "object")
    for (const [k, v] of Object.entries(open))
      if (CONTEXT.test(k) && typeof v === "boolean") kept[k] = v
  return {
    open: kept,
    width:
      typeof width === "number" && Number.isFinite(width)
        ? clampWidth(width)
        : null,
  }
}

export type SheetState = "peek" | "open"
export type SheetEvent =
  { type: "handle" | "escape" } | { type: "drag"; dy: number }

const DRAG_MIN = 40

export function sheetAfter(s: SheetState, e: SheetEvent): SheetState {
  if (e.type === "handle") return s === "open" ? "peek" : "open"
  if (e.type !== "drag") return "peek"
  if (e.dy <= -DRAG_MIN) return "open"
  if (e.dy >= DRAG_MIN) return "peek"
  return s
}
