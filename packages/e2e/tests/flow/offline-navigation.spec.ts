// P6-6: navigating inside Flow while offline must keep Flow on screen (SIMULATED API).
import type { Page } from "@playwright/test"

import { expect, test as base } from "./storefront-api"
import { sc } from "./scenes-api"
import { dock, s } from "./shell"
import { v } from "./voice"

const offline = new WeakMap<Page, boolean>()

/**
 * The shared guard counts every request the tunnel drops as a failure, so this file swaps it
 * for the same checks minus those drops and the console errors Next logs while offline.
 */
const test = base.extend({
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

// The override is not auto like the original, so every test asks for it here.
test.beforeEach(({ api, guard }) => void [api, guard])

/** The train enters a tunnel: every request is dropped and the browser reports offline. */
async function tunnel(page: Page) {
  await page.route("**/*", (route) =>
    offline.get(page) ? route.abort("internetdisconnected") : route.fallback()
  )
  return async (on: boolean) => {
    offline.set(page, on)
    await page.context().setOffline(on)
  }
}

const notice = (page: Page) =>
  page.getByRole("status").filter({ hasText: s.offlineNavigation })

test("Back to home while offline keeps the scene, the draft and says why", async ({
  page,
}) => {
  const flip = await tunnel(page)
  await page.goto("/scenes/orders")
  const back = page.getByRole("button", { name: s.backHome })
  await expect(back).toBeVisible()
  await page.getByLabel(v.message).fill("Draft I am still typing")

  await flip(true)
  await back.click()
  await expect(notice(page)).toBeVisible()
  expect(page.url(), "still in Flow").toMatch(/^http.*\/scenes\/orders$/)
  await expect(dock(page)).toBeVisible()
  await expect(page.getByLabel(v.message)).toHaveValue(
    "Draft I am still typing"
  )

  // Back online the same tap works and the notice goes.
  await flip(false)
  await expect(notice(page)).toBeHidden()
  await back.click()
  await expect(page).toHaveURL(/\/$/)
  await expect(page.getByLabel(v.message)).toHaveValue(
    "Draft I am still typing"
  )
})

test("a Your Flow scene opened while offline keeps Flow on screen", async ({
  page,
}) => {
  const flip = await tunnel(page)
  await page.goto("/")
  const link = page
    .getByRole("region", { name: sc.yourFlow })
    .getByRole("link")
    .first()
  await expect(link).toBeVisible()
  const href = (await link.getAttribute("href"))!

  await flip(true)
  await link.click()
  // Either the scene opens from what is already loaded, or Flow stays and says why.
  await expect
    .poll(async () =>
      new URL(page.url()).pathname === href
        ? "opened"
        : (await notice(page).isVisible())
          ? "notice"
          : "waiting"
    )
    .not.toBe("waiting")
  expect(page.url(), "still in Flow").toMatch(/^http/)
  await expect(dock(page)).toBeVisible()
  test.info().annotations.push({
    type: "offline tap",
    description: new URL(page.url()).pathname === href ? "opened" : "notice",
  })

  await flip(false)
  if (new URL(page.url()).pathname !== href) {
    await link.click()
    await expect(page).toHaveURL(new RegExp(`${href}$`))
  }
})
