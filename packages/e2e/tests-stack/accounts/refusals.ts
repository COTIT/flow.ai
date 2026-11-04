import { test as base, expect } from "@playwright/test"

import { stackAllow } from "../../src/stack"

type Refused = { prefix: string; statuses: number[]; offline?: boolean }

/**
 * The shared guard's checks (console errors, page errors, failed requests, responses >= 400),
 * except that refusals a test sets out to cause pass: `statuses` on any URL under `prefix`.
 * Exact allow URLs can't list them: once a till key is revoked every till call answers 401,
 * timestamped catalog syncs and per-sale lookups included, and a member's overview fires many.
 * `offline` also lets requests fail while the test has the browser offline.
 */
export const test = base.extend<{ refused: Refused }>({
  refused: [{ prefix: "", statuses: [] }, { option: true }],
  page: async ({ page, refused }, use) => {
    const problems: string[] = []
    const allowed = (url: string, status: number) =>
      (refused.prefix !== "" &&
        url.startsWith(refused.prefix) &&
        refused.statuses.includes(status)) ||
      stackAllow.responses.some((a) => a.url === url && a.status === status)
    page.on("response", (r) => {
      if (r.status() >= 400 && !allowed(r.url(), r.status()))
        problems.push(`HTTP ${r.status()} ${r.request().method()} ${r.url()}`)
    })
    page.on("requestfailed", (r) => {
      const why = r.failure()?.errorText ?? ""
      if (why === "net::ERR_ABORTED") return
      if (refused.offline && why === "net::ERR_INTERNET_DISCONNECTED") return
      problems.push(`request failed: ${r.url()} ${why}`)
    })
    page.on("pageerror", (e) => problems.push(`page error: ${e.message}`))
    page.on("console", (m) => {
      if (
        m.type() === "error" &&
        !m.text().startsWith("Failed to load resource:")
      )
        problems.push(`console error: ${m.text()}`)
    })
    await use(page)
    expect(problems, "page problems").toEqual([])
  },
})
export { expect }
