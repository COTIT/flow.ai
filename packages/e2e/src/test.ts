import { test as base, expect, type Page } from "@playwright/test"

/**
 * What a test expects to go wrong. Exact matches only, no patterns:
 * - responses: one URL (absolute, or relative to baseURL) with one status
 * - console: the full text of an expected console error
 * - consolePrefix: the start of an expected console error, for errors whose tail changes per
 *   build (React logs a caught render error with a stack of hashed chunk names); opt-in, per test
 * - failures: one URL whose request is expected to fail (e.g. a dropped connection)
 * e.g. `test.use({ allow: { responses: [{ url: "/b/ZZZZZZZZZZ", status: 404 }] } })`.
 * (An object, not an array: test.use reads an array as `[value, options]`.)
 */
export type Allow = {
  responses?: { url: string; status: number }[]
  console?: string[]
  consolePrefix?: string[]
  failures?: string[]
}

type Options = { allow: Allow }

/**
 * Playwright's `test` with a guard on every page: any console error, uncaught page error,
 * failed request or response >= 400 fails the test, unless listed in `allow`.
 */
export const test = base.extend<Options & { guard: void }>({
  allow: [{}, { option: true }],
  guard: [
    async ({ page, allow, baseURL }, use) => {
      const allowed = (allow.responses ?? []).map((a) => ({
        href: new URL(a.url, baseURL).href,
        status: a.status,
      }))
      const isAllowed = (url: string, status: number) =>
        allowed.some((a) => a.href === url && a.status === status)
      const problems: string[] = []

      page.on("response", (r) => {
        if (r.status() >= 400 && !isAllowed(r.url(), r.status()))
          problems.push(`HTTP ${r.status()} ${r.request().method()} ${r.url()}`)
      })
      const failures = (allow.failures ?? []).map(
        (u) => new URL(u, baseURL).href
      )
      page.on("requestfailed", (r) => {
        if (failures.includes(r.url())) return
        // Navigating away cancels Next's prefetches; that is not a failure.
        if (r.failure()?.errorText !== "net::ERR_ABORTED")
          problems.push(`request failed: ${r.url()} ${r.failure()?.errorText}`)
      })
      page.on("pageerror", (e) => problems.push(`page error: ${e.message}`))
      page.on("console", (m) => {
        if (m.type() !== "error") return
        // Chrome logs every >= 400 response; the response check above already judged it.
        if (m.text().startsWith("Failed to load resource:")) return
        if (allow.console?.includes(m.text())) return
        if (allow.consolePrefix?.some((p) => m.text().startsWith(p))) return
        problems.push(`console error: ${m.text()}`)
      })

      await use()
      expect(problems, "page problems").toEqual([])
    },
    { auto: true },
  ],
})

export { expect }

/** The page is no wider than the viewport (nothing scrolls sideways at 375 px). */
export async function expectNoHorizontalScroll(page: Page) {
  const { scroll, width } = await page.evaluate(() => ({
    scroll: document.documentElement.scrollWidth,
    width: window.innerWidth,
  }))
  expect(scroll, "page scrolls sideways").toBeLessThanOrEqual(width)
}
