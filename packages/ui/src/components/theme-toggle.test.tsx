import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"

import { ThemeToggle } from "./theme-toggle"

describe("ThemeToggle", () => {
  it("takes extra classes for the app's own size", () => {
    const out = renderToStaticMarkup(
      <ThemeToggle label="Switch theme" className="size-10" />
    )
    expect(out).toContain('aria-label="Switch theme"')
    expect(out).toMatch(/class="[^"]*\bsize-10\b/)
    // The app's size replaces the default one instead of fighting it.
    expect(out).not.toMatch(/class="[^"]*\bsize-8\b/)
  })
})
