/**
 * When the floating header shows. It never pushes content: hidden until the pointer reaches the
 * top edge, keyboard focus enters it, the handle is pressed, or (touch, option A) a swipe down
 * starts just below the safe area. It never covers a focused element or an open dialog.
 */
export type HeaderState = {
  shown: boolean
  /** The pointer is over the header. */
  hover: boolean
  /** Keyboard focus is inside the header. */
  focus: boolean
  /** One of its menus (workspace, language) is open. */
  popup: boolean
  /** A modal dialog is open somewhere: nothing reveals the header over it. */
  dialog: boolean
}

export type HeaderEvent =
  | { type: "edge" | "enter" | "leave" | "timeout" }
  | { type: "focusin" | "focusout" | "covers" }
  | { type: "popup" | "dialog"; open: boolean }
  | { type: "handle" | "swipe" | "swipeUp" | "escape" | "outside" | "idle" }

export const HIDDEN: HeaderState = {
  shown: false,
  hover: false,
  focus: false,
  popup: false,
  dialog: false,
}

/** How long after the pointer leaves the header hides (the hook schedules `timeout`). */
export const LEAVE_MS = 600
/** Touch: hidden again after this long without use. */
export const IDLE_MS = 6000

const held = (s: HeaderState) => s.hover || s.focus || s.popup

export function reveal(s: HeaderState, e: HeaderEvent): HeaderState {
  const show = (next: Partial<HeaderState> = {}) =>
    s.dialog ? { ...s, ...next } : { ...s, ...next, shown: true }
  switch (e.type) {
    case "edge":
    case "enter":
      return show({ hover: true })
    case "leave":
      return { ...s, hover: false }
    case "timeout":
      return held(s) ? s : { ...s, shown: false }
    case "focusin":
      return show({ focus: true })
    case "focusout":
      return { ...s, focus: false, shown: s.popup || s.hover ? s.shown : false }
    case "covers":
      return { ...s, focus: false, hover: false, shown: false }
    case "popup":
      return { ...s, popup: e.open }
    case "dialog":
      return e.open
        ? { ...s, dialog: true, shown: false }
        : { ...s, dialog: false }
    case "handle":
      return s.shown ? { ...s, shown: false, hover: false } : show()
    case "swipe":
      return show()
    case "escape":
      return { ...s, shown: false, hover: false, popup: false }
    case "swipeUp":
    case "outside":
    case "idle":
      return s.popup || (e.type === "idle" && (s.focus || s.hover))
        ? s
        : { ...s, shown: false, hover: false }
  }
}

/** Where a reveal swipe may start, below the safe-area top (the OS owns the very edge). */
export const SWIPE_ZONE = 32
const SWIPE_MIN = 32

/** Option A: a mostly vertical swipe down, starting just below the safe area, content at its top. */
export function isRevealSwipe(g: {
  startY: number
  dy: number
  dx: number
  safeTop: number
  atTop: boolean
}) {
  return (
    g.atTop &&
    g.startY >= g.safeTop + 4 &&
    g.startY <= g.safeTop + 4 + SWIPE_ZONE &&
    g.dy >= SWIPE_MIN &&
    Math.abs(g.dx) < g.dy
  )
}
