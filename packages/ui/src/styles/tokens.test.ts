import { readFileSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"

const css = readFileSync(join(import.meta.dirname, "globals.css"), "utf8")

/** `--name: value` pairs declared directly inside `selector { … }`. */
function tokens(selector: string): Record<string, string> {
  const block = css.match(new RegExp(`^${selector} \\{([^}]*)\\}`, "m"))?.[1]
  if (!block) throw new Error(`no ${selector} block in globals.css`)
  return Object.fromEntries(
    [...block.matchAll(/--([\w-]+):\s*([^;]+);/g)].map((m) => [m[1], m[2]])
  )
}

const light = tokens(":root")
const dark = tokens("\\.dark")

/** WCAG 2.2 contrast ratio of two `#rrggbb` colours. */
function contrast(a: string, b: string): number {
  const lum = (hex: string) => {
    const [r, g, b] = [1, 3, 5].map((i) => {
      const c = parseInt(hex.slice(i, i + 2), 16) / 255
      return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
    }) as [number, number, number]
    return 0.2126 * r + 0.7152 * g + 0.0722 * b
  }
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x) as [number, number]
  return (hi + 0.05) / (lo + 0.05)
}

describe("theme tokens", () => {
  it("dark mode sets every colour light mode sets", () => {
    const colours = Object.keys(light).filter((k) => k !== "radius")
    expect(colours.filter((k) => !(k in dark))).toEqual([])
  })

  it("carries the brand colours from the style guide", () => {
    expect(light.primary).toBe("#0a183a")
    expect(dark.background).toBe("#0a183a")
    expect(light.gold).toBeDefined()
    expect(light.teal).toBe("#01909b")
  })

  // Text pairs a page can show; AA = 4.5:1 for normal text.
  const pairs = [
    ["foreground", "background"],
    ["foreground", "muted"],
    ["card-foreground", "card"],
    ["muted-foreground", "background"],
    ["muted-foreground", "muted"],
    ["muted-foreground", "card"],
    ["primary-foreground", "primary"],
    ["secondary-foreground", "secondary"],
    ["accent-foreground", "accent"],
    ["gold-foreground", "gold"],
    ["teal-foreground", "teal"],
    ["destructive", "background"],
  ] as const

  describe.each([
    ["light", light],
    ["dark", dark],
  ])("%s", (_, theme) => {
    it.each(pairs)("%s on %s meets WCAG AA", (fg, bg) => {
      expect(contrast(theme[fg]!, theme[bg]!)).toBeGreaterThanOrEqual(4.5)
    })
  })
})
