import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import { Logo, LogoIcon } from "./logo"

describe("Logo", () => {
  it("is one image named Genesy", () => {
    const html = renderToStaticMarkup(<Logo />)
    expect(html).toMatch(/^<svg[^>]*role="img"/)
    expect(html).toContain('aria-label="Genesy"')
  })

  it("draws the word mark navy, white on dark backgrounds (the two provided files)", () => {
    expect(renderToStaticMarkup(<Logo />)).toContain("dark:fill-white")
    expect(renderToStaticMarkup(<Logo tone="onDark" />)).not.toContain(
      "fill-[#0a183a]"
    )
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
