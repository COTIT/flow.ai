import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import { Logo, LogoIcon } from "./logo"

describe("Logo", () => {
  it("is one image named Verzo", () => {
    const html = renderToStaticMarkup(<Logo />)
    expect(html).toMatch(/^<svg[^>]*role="img"/)
    expect(html).toContain('aria-label="Verzo"')
  })

  it("uses theme colors by default and high-contrast colors on dark backgrounds", () => {
    const auto = renderToStaticMarkup(<Logo />)
    expect(auto).toContain("text-foreground")
    expect(auto).toContain("fill-gold-ink")

    const onDark = renderToStaticMarkup(<Logo tone="onDark" />)
    expect(onDark).toContain("text-white")
    expect(onDark).toContain("fill-[#efc977]")
  })

  it("never renders below the style guide's 100 px minimum", () => {
    expect(renderToStaticMarkup(<Logo />)).toContain("min-w-[100px]")
  })
})

describe("LogoIcon", () => {
  it("is one square image named Verzo", () => {
    const html = renderToStaticMarkup(<LogoIcon />)
    expect(html).toContain('aria-label="Verzo"')
    expect(html).toContain('viewBox="0 0 64 64"')
  })
})
