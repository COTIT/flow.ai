import type { Page } from "@playwright/test"

import { expect } from "./test"

/** Page ground and text from the shared theme tokens (packages/ui globals.css, 2026-11-11). */
const themes = {
  light: { background: "rgb(250, 248, 243)", color: "rgb(28, 27, 24)" },
  dark: { background: "rgb(26, 26, 25)", color: "rgb(245, 244, 240)" },
} as const

/**
 * Opens `path` in light and then dark system mode and checks the page paints the shared
 * warm-light and neutral-dark grounds (glass apps paint `--ground` on <body>, whose last
 * layer is the same colour). `selector` names the element that carries the ground.
 */
export async function expectThemeGrounds(page: Page, path: string, selector = "body") {
  for (const scheme of ["light", "dark"] as const) {
    await page.emulateMedia({ colorScheme: scheme })
    await page.goto(path)
    const html = page.locator("html")
    if (scheme === "dark") await expect(html).toHaveClass(/\bdark\b/)
    else await expect(html).not.toHaveClass(/\bdark\b/)
    const el = page.locator(selector).first()
    await expect(el).toHaveCSS("background-color", themes[scheme].background)
    await expect(el).toHaveCSS("color", themes[scheme].color)
  }
}
