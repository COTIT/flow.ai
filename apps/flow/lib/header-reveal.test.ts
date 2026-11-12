import { describe, expect, it } from "vitest"

import {
  HIDDEN,
  isRevealSwipe,
  reveal,
  type HeaderEvent,
  type HeaderState,
} from "./header-reveal"

const run = (events: HeaderEvent[], from: HeaderState = HIDDEN) =>
  events.reduce(reveal, from)

describe("header reveal", () => {
  it("starts hidden", () => {
    expect(HIDDEN.shown).toBe(false)
  })

  it("shows when the pointer reaches the top edge, hides after it leaves", () => {
    expect(run([{ type: "edge" }]).shown).toBe(true)
    expect(run([{ type: "edge" }, { type: "leave" }]).shown).toBe(true)
    expect(
      run([{ type: "edge" }, { type: "leave" }, { type: "timeout" }]).shown
    ).toBe(false)
  })

  it("stays while the pointer came back before the timeout", () => {
    const s = run([
      { type: "edge" },
      { type: "leave" },
      { type: "enter" },
      { type: "timeout" },
    ])
    expect(s.shown).toBe(true)
  })

  it("stays while focus is inside, and while one of its menus is open", () => {
    const focused = run([
      { type: "focusin" },
      { type: "leave" },
      { type: "timeout" },
    ])
    expect(focused.shown).toBe(true)
    const menu = run([
      { type: "edge" },
      { type: "popup", open: true },
      { type: "leave" },
      { type: "timeout" },
      { type: "idle" },
      { type: "outside" },
    ])
    expect(menu.shown).toBe(true)
    expect(
      run([{ type: "popup", open: false }, { type: "timeout" }], menu).shown
    ).toBe(false)
  })

  it("keyboard focus reveals it; leaving with focus hides it", () => {
    expect(run([{ type: "focusin" }]).shown).toBe(true)
    expect(run([{ type: "focusin" }, { type: "focusout" }]).shown).toBe(false)
  })

  it("never covers a focused element or an open dialog", () => {
    expect(run([{ type: "edge" }, { type: "covers" }]).shown).toBe(false)
    const dialog = run([{ type: "edge" }, { type: "dialog", open: true }])
    expect(dialog.shown).toBe(false)
    // While a dialog is open nothing brings it back.
    expect(
      run([{ type: "edge" }, { type: "handle" }, { type: "swipe" }], dialog)
        .shown
    ).toBe(false)
    expect(
      run([{ type: "dialog", open: false }, { type: "edge" }], dialog).shown
    ).toBe(true)
  })

  it("the handle toggles; swipe up, Escape, a tap outside and idle hide", () => {
    expect(run([{ type: "handle" }]).shown).toBe(true)
    expect(run([{ type: "handle" }, { type: "handle" }]).shown).toBe(false)
    for (const type of ["swipeUp", "escape", "outside", "idle"] as const)
      expect(run([{ type: "swipe" }, { type }]).shown).toBe(false)
  })

  it("idle never hides it while focus is inside", () => {
    expect(run([{ type: "focusin" }, { type: "idle" }]).shown).toBe(true)
  })

  it("Escape hides it even with focus inside", () => {
    expect(run([{ type: "focusin" }, { type: "escape" }]).shown).toBe(false)
  })
})

describe("isRevealSwipe (option A)", () => {
  const base = { startY: 30, dy: 60, dx: 0, safeTop: 20 }
  it("a downward swipe starting just below the safe area reveals", () => {
    expect(isRevealSwipe(base)).toBe(true)
  })
  it("starting inside the safe area (the OS edge) does not", () => {
    expect(isRevealSwipe({ ...base, startY: 10 })).toBe(false)
  })
  it("starting far below the top does not", () => {
    expect(isRevealSwipe({ ...base, startY: 120 })).toBe(false)
  })
  it("a short or sideways move does not", () => {
    expect(isRevealSwipe({ ...base, dy: 12 })).toBe(false)
    expect(isRevealSwipe({ ...base, dx: 80 })).toBe(false)
  })
  it("a swipe that starts in the reveal zone reveals wherever the content is scrolled (P10-6)", () => {
    expect(isRevealSwipe(base)).toBe(true)
  })
})
