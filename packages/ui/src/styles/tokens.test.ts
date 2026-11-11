import { readFileSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"

import { pwaViewport } from "../lib/pwa"

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
// Office, shop and till put `theme-glass` on <body>; landing and field never do.
const glassLight = tokens("\\.theme-glass")
const glassDark = tokens("\\.dark \\.theme-glass")

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

  // Approved "Shared theme tokens" board (2026-11-11): neutral near-black dark, warm light.
  it("uses the warm light and neutral dark grounds from the approved board", () => {
    expect(light).toMatchObject({
      background: "#faf8f3",
      foreground: "#1c1b18",
      card: "#ffffff",
      primary: "#1c1b18",
      muted: "#f3efe6",
      "muted-foreground": "#5f5c55",
      border: "#e4dfd6",
      gold: "#d0a55f",
      "gold-ink": "#8a6116",
    })
    expect(dark).toMatchObject({
      background: "#1a1a19",
      foreground: "#f5f4f0",
      card: "#262624",
      primary: "#efc977",
      muted: "#30302e",
      "muted-foreground": "#a8a69e",
      sidebar: "#1f1f1d",
      gold: "#efc977",
      "gold-ink": "#efc977",
    })
    expect(light.teal).toBe("#01909b")
  })

  it("leaves no navy in any token block", () => {
    const navy = /#0a183a|#13234d|#1b2c57|#456087|#3a5378|#8fa6c8|#b4c3d3|#d1dce5|#f2f6f7|#eef3f5|#f8fafb|rgb\((10 24 58|27 44 87|19 35 77|69 96 135|143 166 200)\b/i
    for (const block of [light, dark, glassLight, glassDark])
      expect(Object.values(block).filter((v) => navy.test(v))).toEqual([])
    expect(css).not.toMatch(navy)
  })

  it("colours the browser bar like the page background", () => {
    expect(pwaViewport.themeColor).toEqual([
      { media: "(prefers-color-scheme: light)", color: light.background },
      { media: "(prefers-color-scheme: dark)", color: dark.background },
    ])
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
    ["gold-ink", "background"],
    ["gold-ink", "card"],
    ["info", "card"],
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

describe("glass theme tokens", () => {
  const names = [
    "radius",
    "ground",
    "ground-base",
    "glass",
    "glass-strong",
    "glass-subtle",
    "glass-solid",
    "glass-border",
    "glass-highlight",
    "glass-shadow",
    "glass-blur",
    "field",
    "field-border",
    "hair",
    "hair-2",
    "tint-info",
    "tint-success",
    "tint-warning",
    "tint-error",
  ]

  it("light sets every glass token", () => {
    expect(names.filter((n) => !(n in glassLight))).toEqual([])
  })

  it("dark sets every colour light sets", () => {
    const colours = Object.keys(glassLight).filter(
      (k) => !["radius", "glass-blur"].includes(k)
    )
    expect(colours.filter((k) => !(k in glassDark))).toEqual([])
  })

  it("rounds corners more than the base theme", () => {
    expect(glassLight.radius).toBe("0.75rem")
  })

  it("keeps blur within the speed budget (16px max)", () => {
    expect(parseFloat(glassLight["glass-blur"]!)).toBeLessThanOrEqual(16)
  })

  // Text must stay readable where blur is off (no support, or "reduce transparency").
  describe.each([
    ["light", light, glassLight],
    ["dark", dark, glassDark],
  ])("%s", (_, base, glass) => {
    it("the info tone on the solid glass fallback meets WCAG AA", () => {
      expect(contrast(glass["tone-info"]!, glass["glass-solid"]!)).toBeGreaterThanOrEqual(4.5)
    })
    it.each(["foreground", "muted-foreground", "card-foreground", "gold-ink"])(
      "%s on the solid glass fallback meets WCAG AA",
      (fg) => {
        expect(contrast(base[fg]!, glass["glass-solid"]!)).toBeGreaterThanOrEqual(4.5)
      }
    )
    it.each(["foreground", "muted-foreground"])(
      "%s on the ground colour meets WCAG AA",
      (fg) => {
        expect(contrast(base[fg]!, glass["ground-base"]!)).toBeGreaterThanOrEqual(4.5)
      }
    )
  })
})

describe("glass button sizes reach links styled with buttonVariants()", () => {
  // `<Link className={buttonVariants()}>` and base-ui `render` triggers have no data-slot,
  // only the `group/button` class, so the sizes must key on that class.
  it("button size rules key on the group/button class, never on data-slot alone", () => {
    expect(css).not.toMatch(/\.theme-glass[^{]*\[data-slot="button"\]/)
    expect(css.match(/\.theme-glass [^{]*\.group\\\/button/g)?.length ?? 0).toBeGreaterThan(5)
  })
})
