// Shared by the offline specs (P6-6, J3): a tunnel that drops every request and a page guard
// that does not count those drops (SIMULATED API).
import type { Page } from "@playwright/test"

import { expect, test as base } from "./storefront-api"
import { s } from "./shell"

const offline = new WeakMap<Page, boolean>()

/**
 * The shared guard counts every request the tunnel drops as a failure, so this file swaps it
 * for the same checks minus those drops and the console errors Next logs while offline.
 */
export const test = base.extend({
  guard: async ({ page }, use) => {
    const problems: string[] = []
    page.on("pageerror", (e) => problems.push(`page error: ${e.message}`))
    page.on("console", (m) => {
      if (m.type() !== "error") return
      if (m.text().startsWith("Failed to load resource:")) return
      if (offline.get(page)) return // Next reports the fetch it could not make
      problems.push(`console error: ${m.text()}`)
    })
    page.on("requestfailed", (r) => {
      // Only the tunnel drops requests this way; one in flight may land just after it ends.
      if (r.failure()?.errorText === "net::ERR_INTERNET_DISCONNECTED") return
      if (r.failure()?.errorText !== "net::ERR_ABORTED")
        problems.push(`request failed: ${r.url()} ${r.failure()?.errorText}`)
    })
    page.on("response", (r) => {
      if (r.status() >= 400)
        problems.push(`HTTP ${r.status()} ${r.request().method()} ${r.url()}`)
    })
    await use()
    expect(problems, "page problems").toEqual([])
  },
})

/** The train enters a tunnel: every request is dropped and the browser reports offline. */
export async function tunnel(page: Page) {
  await page.route("**/*", (route) =>
    offline.get(page) ? route.abort("internetdisconnected") : route.fallback()
  )
  return async (on: boolean) => {
    offline.set(page, on)
    await page.context().setOffline(on)
  }
}

export const notice = (page: Page) =>
  page.getByRole("status").filter({ hasText: s.offlineNavigation })

export { expect }
