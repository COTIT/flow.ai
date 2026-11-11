// P3-5: online at the tap, the network drops while the next page's code is still loading.
// The failed page must keep Flow around it (header, dock, draft) and recover once back online.
// SIMULATED API (storefront-api) and network (routes + context offline).
import type { Page } from "@playwright/test"

import { expectNoHorizontalScroll } from "../../src/test"
import { designs, expect, test as base } from "./storefront-api"
import { dock, revealHeader } from "./shell"
import { sc } from "./scenes-api"
import { v } from "./voice"

const down = new WeakMap<Page, boolean>()

/** As offline-navigation.spec: the shared guard minus the drops this file causes on purpose. */
const test = base.extend({
  guard: async ({ page }, use) => {
    const problems: string[] = []
    page.on("pageerror", (e) => problems.push(`page error: ${e.message}`))
    page.on("console", (m) => {
      if (m.type() !== "error") return
      if (m.text().startsWith("Failed to load resource:")) return
      if (down.get(page)) return // React and Next report the code that did not arrive
      problems.push(`console error: ${m.text()}`)
    })
    page.on("requestfailed", (r) => {
      if (["net::ERR_INTERNET_DISCONNECTED", "net::ERR_ABORTED"].includes(r.failure()?.errorText ?? "")) return
      problems.push(`request failed: ${r.url()} ${r.failure()?.errorText}`)
    })
    page.on("response", (r) => {
      if (r.status() >= 400) problems.push(`HTTP ${r.status()} ${r.request().method()} ${r.url()}`)
    })
    await use()
    expect(problems, "page problems").toEqual([])
  },
})
test.beforeEach(({ api, guard }) => void [api, guard])

const DRAFT = "Half a thought about the shop"
const failed = (page: Page) => page.getByText("This page couldn't load")

async function openStorefront(page: Page) {
  await revealHeader(page)
  await page.getByRole("link", { name: "Storefront", exact: true }).click()
}

/**
 * Online at the tap: the page's code is cut off as it starts loading (the browser still says
 * online), then the network is gone. Returns `back()` to bring it back.
 */
async function dropMidNavigation(page: Page, { offline = true, open = openStorefront } = {}) {
  down.set(page, true)
  await page.route("**/_next/static/**", (route) =>
    down.get(page) ? route.abort("internetdisconnected") : route.fallback()
  )
  await open(page)
  if (offline) await page.context().setOffline(true)
  return async () => {
    down.set(page, false)
    if (offline) await page.context().setOffline(false)
  }
}

async function start(page: Page, api: Parameters<typeof designs>[0]) {
  designs(api)
  await page.goto("/")
  await page.getByLabel(v.message).fill(DRAFT)
}

test("P3-5 a page that couldn't load keeps Flow around it and loads once back online", async ({ page, api }) => {
  await start(page, api)
  const back = await dropMidNavigation(page)
  await expect(failed(page)).toBeVisible()
  // Flow stays: the header, the dock with the draft, a way home. Never "Nothing was changed" as a crash.
  await expect(page.getByRole("banner")).toBeAttached()
  await expect(dock(page)).toBeVisible()
  await expect(page.getByLabel(v.message)).toHaveValue(DRAFT)
  await expect(page.getByRole("link", { name: "Go to the start" })).toBeVisible()
  await expect(page.getByText("This screen stopped working")).toHaveCount(0)
  await expectNoHorizontalScroll(page)
  await page.waitForTimeout(2_500)
  await back() // the `online` event retries once by itself
  await expect(page.getByRole("heading", { level: 1, name: "Storefront design" })).toBeVisible()
  await expect(page).toHaveURL(/\/storefront$/)
  await expect(dock(page)).toBeVisible()
  await expect(page.getByLabel(v.message)).toHaveValue(DRAFT)
})

test("P3-5 Try again offline stays put and says why; online it loads the page", async ({ page, api }) => {
  await start(page, api)
  const back = await dropMidNavigation(page)
  await expect(failed(page)).toBeVisible()
  const retry = page.getByRole("button", { name: "Try again" })
  await retry.click()
  await expect(failed(page)).toBeVisible()
  await expect(page.getByText("You're offline. This page loads again once you're back online.")).toBeVisible()
  await expect(dock(page)).toBeVisible()
  await expect(page.getByLabel(v.message)).toHaveValue(DRAFT)

  // Back online: the `online` event loads it by itself.
  await back()
  await expect(page.getByRole("heading", { level: 1, name: "Storefront design" })).toBeVisible()
  await expect(page.getByLabel(v.message)).toHaveValue(DRAFT)
})

test("P3-5 the code failed but the browser stayed online: Try again loads that page for real", async ({ page, api }) => {
  await start(page, api)
  const back = await dropMidNavigation(page, { offline: false })
  await expect(failed(page)).toBeVisible()
  await expect(page.getByText("Check your connection, then try again. Nothing was changed.")).toBeVisible()
  await back() // no `online` event: only Try again can bring the page
  await page.getByRole("button", { name: "Try again" }).click()
  await expect(page.getByRole("heading", { level: 1, name: "Storefront design" })).toBeVisible()
  await expect(page).toHaveURL(/\/storefront$/)
  await expect(dock(page)).toBeVisible()
  await expect(page.getByLabel(v.message)).toHaveValue(DRAFT)
})

test("P3-5 a scene whose code never arrived offers a real reload, not the same failure again", async ({ page, api }) => {
  await start(page, api)
  const back = await dropMidNavigation(page, {
    offline: false,
    open: (p) => p.getByRole("region", { name: sc.yourFlow }).getByRole("link").first().click(),
  })
  await expect(failed(page)).toBeVisible()
  await expect(page).toHaveURL(/\/scenes\//)
  await back()
  await page.getByRole("button", { name: "Try again" }).click()
  await expect(failed(page)).toHaveCount(0)
  await expect(page.getByText(sc.sceneError)).toHaveCount(0)
  await expect(page).toHaveURL(/\/scenes\//)
  await expect(page.getByLabel(v.message)).toHaveValue(DRAFT)
})
