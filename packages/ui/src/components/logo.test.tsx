import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import { Logo, LogoIcon } from "./logo"

describe("Logo", () => {
  it("is one image named Genesy", () => {
    const html = renderToStaticMarkup(<Logo />)
    expect(html).toMatch(/^<svg[^>]*role="img"/)
    expect(html).toContain('aria-label="Genesy"')
  })

  it("draws the word mark in the theme's text colour, white on dark backgrounds", () => {
    const auto = renderToStaticMarkup(<Logo />)
    expect(auto).toContain("fill-foreground")
    expect(auto).not.toMatch(/fill-\[#/)
    expect(renderToStaticMarkup(<Logo tone="onDark" />)).not.toContain(
      "fill-foreground"
    )
  })

  it("gives every copy its own gradients, so a hidden copy cannot blank a visible one", () => {
    const html = renderToStaticMarkup(
      <>
        <Logo />
        <Logo />
        <LogoIcon />
      </>
    )
    const ids = [...html.matchAll(/ id="([^"]+)"/g)].map((m) => m[1])
    expect(ids.length).toBeGreaterThan(0)
    expect(new Set(ids).size).toBe(ids.length)
    for (const [, ref] of html.matchAll(/(?:url\(#|href="#)([^)"]+)/g))
      expect(ids).toContain(ref)
  })

  it("never renders below the style guide's 100 px minimum", () => {
    expect(renderToStaticMarkup(<Logo />)).toContain("min-w-[100px]")
  })
})

describe("LogoIcon", () => {
  it("is one image named Genesy, at least 42 px", () => {
    const html = renderToStaticMarkup(<LogoIcon />)
    expect(html).toContain('aria-label="Genesy"')
    expect(html).toContain("min-w-[42px]")
  })
})
