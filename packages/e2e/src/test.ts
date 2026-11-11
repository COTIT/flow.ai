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
 * Failures a test causes on purpose (offline, a 503 it routes), per page: the guard lets requests
 * matching these through; everything else still fails the test.
 */
const injected = new WeakMap<Page, RegExp[]>()
export function markInjected(page: Page, url: RegExp) {
  injected.set(page, [...(injected.get(page) ?? []), url])
}
const isInjected = (page: Page, url: string) =>
  injected.get(page)?.some((re) => re.test(url)) ?? false

/** Records a page's problems (console errors, page errors, failed or >= 400 requests). */
export function watchProblems(page: Page, allow: Allow, baseURL?: string) {
  const allowed = (allow.responses ?? []).map((a) => ({
    href: new URL(a.url, baseURL).href,
    status: a.status,
  }))
  const isAllowed = (url: string, status: number) =>
    allowed.some((a) => a.href === url && a.status === status)
  const problems: string[] = []

  page.on("response", (r) => {
    if (r.status() >= 400 && !isAllowed(r.url(), r.status()) && !isInjected(page, r.url()))
      problems.push(`HTTP ${r.status()} ${r.request().method()} ${r.url()}`)
  })
  const failures = (allow.failures ?? []).map(
    (u) => new URL(u, baseURL).href
  )
  page.on("requestfailed", (r) => {
    if (failures.includes(r.url()) || isInjected(page, r.url())) return
    // Navigating away cancels Next's prefetches; that is not a failure.
    if (r.failure()?.errorText !== "net::ERR_ABORTED")
      problems.push(`request failed: ${r.url()} ${r.failure()?.errorText}`)
  })
  page.on("pageerror", (e) => {
    // The browser's own offline error page: its scripts are not the app's (e.g. a reload offline).
    if (page.url().startsWith("chrome-error://")) return
    // A code chunk that failed to load because the test cut that request (offline, failApi).
    const chunk = e.message.match(/Failed to load chunk (\S+)/)?.[1]
    if (chunk && isInjected(page, new URL(chunk, page.url()).href)) return
    problems.push(`page error: ${e.message}`)
  })
  page.on("console", (m) => {
    if (m.type() !== "error") return
    // Chrome logs every >= 400 response; the response check above already judged it.
    if (m.text().startsWith("Failed to load resource:")) return
    if (allow.console?.includes(m.text())) return
    if (allow.consolePrefix?.some((p) => m.text().startsWith(p))) return
    problems.push(`console error: ${m.text()}`)
  })
  return problems
}

/**
 * Playwright's `test` with a guard on every page: any console error, uncaught page error,
 * failed request or response >= 400 fails the test, unless listed in `allow`.
 */
export const test = base.extend<Options & { guard: void }>({
  allow: [{}, { option: true }],
  guard: [
    async ({ page, allow, baseURL }, use) => {
      const problems = watchProblems(page, allow, baseURL)
      await use()
      expect(problems, "page problems").toEqual([])
      // Flow's shell has one scroll root; every Flow test ends with the layout checked.
      if (
        !page.isClosed() &&
        (await page.locator("[data-flow-scroll]").count())
      )
        await expectSingleScroller(page)
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

/**
 * Split-view scrolling. The document itself never scrolls (no space below the viewport, nothing
 * sideways). Only the scroll `root` (e.g. Flow's `main[data-flow-scroll]`) and, unless `strict`,
 * elements marked `[data-flow-pane]` (side-by-side panes: results, conversation rail, sheet body)
 * may scroll vertically; no scroller sits inside another, so each pane has exactly one.
 */
export async function expectSingleScroller(
  page: Page,
  root = "[data-flow-scroll]",
  { strict = false }: { strict?: boolean } = {}
) {
  const found = await page.evaluate(
    ({ root, strict }) => {
      const d = document.documentElement
      const scrolls = (el: Element) => {
        const y = getComputedStyle(el).overflowY
        return /(auto|scroll)/.test(y) && el.scrollHeight > el.clientHeight + 1
      }
      const allowed = (el: Element) =>
        el.matches(root) || (!strict && el.matches("[data-flow-pane]"))
      const name = (el: Element) =>
        `${el.tagName.toLowerCase()}.${String(el.className).slice(0, 60)}`
      const scrollers = Array.from(
        document.querySelectorAll<HTMLElement>("body *")
      ).filter(scrolls)
      return {
        page: d.scrollHeight - window.innerHeight,
        sideways: d.scrollWidth - window.innerWidth,
        others: scrollers.filter((el) => !allowed(el)).map(name),
        nested: scrollers
          .filter((el) => scrollers.some((o) => o !== el && o.contains(el)))
          .map(name),
      }
    },
    { root, strict }
  )
  expect(found.page, "page scrolls below the viewport").toBeLessThanOrEqual(0)
  expect(found.sideways, "page scrolls sideways").toBeLessThanOrEqual(0)
  expect(found.others, "a scroll bar outside the panes").toEqual([])
  expect(found.nested, "a scroller inside another").toEqual([])
}

/**
 * Every visible control is at least `min` px wide and tall (touch screens; the till asks for 44).
 * `within` limits the check to part of the page, e.g. `{ within: "main" }`.
 */
export async function expectTapTargets(
  page: Page,
  { min = 44, within = "body" }: { min?: number; within?: string } = {}
) {
  // Measure settled boxes: a sheet scaling in reports its buttons a fraction too small.
  // Endless animations (skeleton pulse) never finish, so only finite ones are awaited.
  await page.evaluate(() =>
    Promise.all(
      document
        .getAnimations()
        .filter((a) => a.effect?.getComputedTiming().endTime !== Infinity)
        .map((a) => a.finished.catch(() => undefined))
    )
  )
  const small = await page.locator(within).first().evaluate((root, min) => {
    const controls = root.querySelectorAll<HTMLElement>(
      'a[href], button, input:not([type="hidden"]), select, textarea, summary, [role="button"], [role="tab"], [role="checkbox"], [role="switch"], [role="menuitem"]'
    )
    return [...controls].flatMap((el) => {
      const box = el.getBoundingClientRect()
      // Hidden from users (e.g. a Select's form input behind its trigger): not a target.
      if (el.closest('[aria-hidden="true"], [inert]')) return []
      const shown = box.width > 0 && box.height > 0 && getComputedStyle(el).visibility !== "hidden"
      // A control may widen its hit area with an absolutely placed ::after (switch, checkbox).
      const after = getComputedStyle(el, "::after")
      const grow = (a: string, b: string) =>
        after.content !== "none" && after.position === "absolute" ? -(parseFloat(a) || 0) - (parseFloat(b) || 0) : 0
      const width = Math.max(box.width, box.width + grow(after.left, after.right))
      const height = Math.max(box.height, box.height + grow(after.top, after.bottom))
      if (!shown || (width >= min && height >= min)) return []
      const name = el.getAttribute("aria-label") || el.textContent?.trim() || el.tagName.toLowerCase()
      return [`${name} (${Math.round(width)}×${Math.round(height)})`]
    })
  }, min)
  expect(small, `tap targets under ${min}px`).toEqual([])
}

/**
 * Nothing on the page moved after it first drew (cumulative layout shift, shifts right after
 * a tap or key press excluded). Call once the page has settled.
 */
export async function expectNoLayoutShift(page: Page, max = 0.01) {
  const shift = await page.evaluate(
    () =>
      new Promise<number>((resolve) => {
        let sum = 0
        const observer = new PerformanceObserver((list) => {
          for (const entry of list.getEntries() as (PerformanceEntry & {
            value: number
            hadRecentInput: boolean
          })[])
            if (!entry.hadRecentInput) sum += entry.value
        })
        observer.observe({ type: "layout-shift", buffered: true })
        setTimeout(() => {
          observer.disconnect()
          resolve(sum)
        }, 50)
      })
  )
  expect(shift, "layout shift (CLS)").toBeLessThanOrEqual(max)
}

/**
 * No animated island is on the page (see `island` in @genesy/ui). Use with
 * `test.use({ reducedMotion: "reduce" })` to prove nothing animates, or is fetched, there.
 */
export async function expectNoMotion(page: Page) {
  await expect(page.locator("[data-motion]"), "animated islands").toHaveCount(0)
}

// Internal values that should have reached the screen as a label, a badge or a formatted date.
const RAW_PATTERNS: [string, RegExp][] = [
  ["scope key", /\b[a-z]+:[a-z_]+\b/],
  ["event name", /\b[a-z]+\.[a-z_]{3,}\b/],
  // A missing translation prints its key: `invoices.statuses.pending`, `orders.makeInvoice`.
  ["i18n key", /\b[a-z][a-zA-Z]+(?:\.[a-zA-Z][a-zA-Z0-9_]+)+\b/],
  ["snake_case", /\b[a-z0-9]+(?:_[a-z0-9]+)+\b/],
  // API decimal text that skipped formatting: `118.82000000`, `30.4733`.
  ["raw decimal", /\b\d+[.,]\d{4,}\b/],
  ["UUID", /\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b/i],
  ["boolean", /^(?:true|false)$/],
  ["ISO date", /\b\d{4}-\d{2}-\d{2}(?!\d)/],
  ["US date", /\b\d{1,2}\/\d{1,2}\/\d{4}\b/],
]
// Readable text that looks like a pattern above: links, mail addresses, host names, key prefixes.
const NOT_RAW = [
  /\b[a-z][a-z0-9+.-]*:\/\/\S+/gi,
  /\S+@\S+\.\S+/g,
  /\b[\w-]+(?:\.[\w-]+)*\.(?:app|com|eu|org|net|io|nl|tz|co|dev|test|localhost)\b/gi,
  /\bgsk_\w+/g,
]

/**
 * No visible text is a raw internal value: a scope key (`catalog:read`), an event name
 * (`stock.changed`), a translation key (`orders.makeInvoice`), an unformatted decimal (`118.82000000`), a snake_case code, a UUID, a bare `true`/`false`, or an ISO or US machine
 * date. Inputs, `code`/`pre`/`kbd` and anything marked `data-raw` are skipped; `allow` lets legit
 * codes through (exact strings or patterns), `within` limits the check to part of the page.
 */
export async function expectNoRawValues(
  page: Page,
  { allow = [], within = "body" }: { allow?: (string | RegExp)[]; within?: string } = {}
) {
  const texts = await page.locator(within).first().evaluate((root) => {
    const skip = 'input, textarea, select, option, code, pre, kbd, script, style, template, [data-raw], [aria-hidden="true"]'
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT)
    const out: string[] = []
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      const el = n.parentElement
      const text = n.textContent?.trim()
      if (!el || !text || el.closest(skip) || !el.checkVisibility()) continue
      out.push(text)
    }
    return out
  })
  const cleared = [...NOT_RAW, ...allow]
  const raw = texts.flatMap((text) => {
    const left = cleared.reduce<string>(
      (t, a) => (typeof a === "string" ? t.split(a).join(" ") : t.replace(new RegExp(a.source, "g" + a.flags.replace("g", "")), " ")),
      text
    ).trim()
    const hit = RAW_PATTERNS.find(([, re]) => re.test(left))
    return hit ? [`${hit[0]}: "${text}"`] : []
  })
  expect(raw, "raw values on screen").toEqual([])
}
