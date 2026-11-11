import { describe, expect, it } from "vitest"

import { orbMode, slotTransform, springEasing } from "./orb-geometry"

const rect = (left: number, top: number, width: number, height: number) => ({
  left,
  top,
  width,
  height,
})

describe("orb mode", () => {
  it("is compact on every scene, immersive or ambient on home", () => {
    expect(orbMode({ pathname: "/storefront" })).toBe("compact")
    expect(orbMode({ pathname: "/storefront", immersive: true })).toBe(
      "compact"
    )
    expect(orbMode({ pathname: "/", resultOpen: true, immersive: true })).toBe(
      "compact"
    )
    expect(orbMode({ pathname: "/", immersive: true })).toBe("immersive")
    // Once the conversation has content the orb steps aside: small, the chat gets the height.
    expect(orbMode({ pathname: "/", immersive: true, conversing: true })).toBe(
      "compact"
    )
    expect(orbMode({ pathname: "/", conversing: true })).toBe("ambient")
    expect(orbMode({ pathname: "/" })).toBe("ambient")
  })
})

describe("slot transform", () => {
  it("places the base-size orb on the slot, relative to the layer", () => {
    expect(
      slotTransform(rect(140, 100, 320, 320), rect(40, 60, 1200, 700), 320)
    ).toEqual({ x: 100, y: 40, scale: 1 })
  })

  it("scales down and centres in a non-square slot", () => {
    expect(
      slotTransform(rect(1000, 80, 48, 64), rect(0, 60, 1200, 700), 320)
    ).toEqual({ x: 1000, y: 28, scale: 0.15 })
  })
})

describe("spring easing", () => {
  it("is a CSS linear() curve from 0 to 1 that never goes back", () => {
    const css = springEasing()
    expect(css).toMatch(/^linear\(0, .*, 1\)$/)
    const points = css.slice(7, -1).split(", ").map(Number)
    expect(points.length).toBeGreaterThan(10)
    for (let i = 1; i < points.length; i++)
      expect(points[i]).toBeGreaterThanOrEqual(points[i - 1]!)
    // Critically damped: most of the way by half time, no overshoot.
    expect(points[Math.floor(points.length / 2)]).toBeGreaterThan(0.8)
    expect(Math.max(...points)).toBe(1)
  })
})
