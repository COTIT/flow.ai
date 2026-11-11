import { describe, expect, it } from "vitest"

import {
  RAIL,
  band,
  clampWidth,
  keyWidth,
  railContext,
  railOpen,
  readRail,
  sheetAfter,
} from "./rail"

describe("band", () => {
  it("phone below 640, tablet to 1023, desktop from 1024", () => {
    expect(band(375)).toBe("phone")
    expect(band(639)).toBe("phone")
    expect(band(640)).toBe("tablet")
    expect(band(1023)).toBe("tablet")
    expect(band(1024)).toBe("desktop")
  })
})

describe("rail open", () => {
  it("desktop: open by default; tablet: closed on a scene, open during a review", () => {
    expect(railOpen({ band: "desktop", review: false, stored: {} })).toBe(true)
    expect(railOpen({ band: "tablet", review: false, stored: {} })).toBe(false)
    expect(railOpen({ band: "tablet", review: true, stored: {} })).toBe(true)
  })
  it("the person's choice wins, per context", () => {
    const stored = { [railContext("tablet", false)]: true }
    expect(railOpen({ band: "tablet", review: false, stored })).toBe(true)
    // Hiding it on a scene does not hide it during the next review.
    const hid = { [railContext("tablet", false)]: false }
    expect(railOpen({ band: "tablet", review: true, stored: hid })).toBe(true)
  })
  it("never on a phone (the sheet takes over)", () => {
    expect(railOpen({ band: "phone", review: true, stored: {} })).toBe(false)
  })
})

describe("rail width", () => {
  it("defaults 400 on desktop, 340 on tablet", () => {
    expect(RAIL.desktop).toBe(400)
    expect(RAIL.tablet).toBe(340)
  })
  it("clamps to 320–520", () => {
    expect(clampWidth(100)).toBe(320)
    expect(clampWidth(900)).toBe(520)
    expect(clampWidth(410.6)).toBe(411)
  })
  it("keyboard: left widens, right narrows, Home/End go to the ends", () => {
    expect(keyWidth(400, "ArrowLeft")).toBe(420)
    expect(keyWidth(400, "ArrowRight")).toBe(380)
    expect(keyWidth(510, "ArrowLeft")).toBe(520)
    expect(keyWidth(400, "Home")).toBe(320)
    expect(keyWidth(400, "End")).toBe(520)
    expect(keyWidth(400, "Enter")).toBeNull()
  })
})

describe("readRail (per tab, never trusted)", () => {
  it("keeps only known booleans and a clamped width", () => {
    expect(
      readRail('{"open":{"desktop:scene":false,"x":"y"},"width":9999}')
    ).toEqual({
      open: { "desktop:scene": false },
      width: 520,
    })
  })
  it("falls back on missing or broken storage", () => {
    expect(readRail(null)).toEqual({ open: {}, width: null })
    expect(readRail("{not json")).toEqual({ open: {}, width: null })
    expect(readRail('"x"')).toEqual({ open: {}, width: null })
  })
})

describe("phone sheet", () => {
  it("handle toggles; a swipe up opens, a swipe down lowers; small moves keep it", () => {
    expect(sheetAfter("peek", { type: "handle" })).toBe("open")
    expect(sheetAfter("open", { type: "handle" })).toBe("peek")
    expect(sheetAfter("peek", { type: "drag", dy: -60 })).toBe("open")
    expect(sheetAfter("open", { type: "drag", dy: 60 })).toBe("peek")
    expect(sheetAfter("open", { type: "drag", dy: 12 })).toBe("open")
  })
  it("Escape lowers it", () => {
    expect(sheetAfter("open", { type: "escape" })).toBe("peek")
    expect(sheetAfter("peek", { type: "escape" })).toBe("peek")
  })
})
