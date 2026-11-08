import type { Page } from "@playwright/test"

import { expect } from "./test"

/**
 * The page offers a home-screen install: a manifest Android accepts (standalone, start page,
 * 192/512/maskable PNG icons that load) and the iPhone tags Safari reads instead.
 */
export async function expectInstallable(
  page: Page,
  { name, shortName, startUrl }: { name: string; shortName: string; startUrl: string }
) {
  const href = await page.locator('link[rel="manifest"]').getAttribute("href")
  expect(href, "manifest link").toBeTruthy()
  const res = await page.request.get(href!)
  expect(res.status()).toBe(200)
  const manifest = await res.json()
  expect(manifest).toMatchObject({
    name,
    short_name: shortName,
    start_url: startUrl,
    display: "standalone",
  })
  const sizes = manifest.icons.map((i: { sizes: string; purpose?: string }) =>
    `${i.sizes}${i.purpose ? ` ${i.purpose}` : ""}`
  )
  expect(sizes).toEqual(expect.arrayContaining(["192x192", "512x512", "512x512 maskable"]))

  const apple = await page.locator('link[rel="apple-touch-icon"]').getAttribute("href")
  for (const src of [...manifest.icons.map((i: { src: string }) => i.src), apple!]) {
    const icon = await page.request.get(src)
    expect(icon.status(), src).toBe(200)
    expect(icon.headers()["content-type"], src).toBe("image/png")
  }
  await expect(page.locator('meta[name="theme-color"]').first()).toHaveAttribute("content", /^#/)
  await expect(page.locator('meta[name="apple-mobile-web-app-title"]')).toHaveAttribute(
    "content",
    shortName
  )
  await expect(
    page.locator('meta[name="mobile-web-app-capable"], meta[name="apple-mobile-web-app-capable"]').first()
  ).toHaveAttribute("content", "yes")
}

/** Waits until the app's service worker has installed (precache done) and controls the page. */
export async function waitForServiceWorker(page: Page) {
  await page.evaluate(() => navigator.serviceWorker.ready.then(() => true))
  await page.waitForFunction(() => navigator.serviceWorker.controller !== null)
}

/** Every URL the service worker has kept, across all its caches. */
export function cachedUrls(page: Page): Promise<string[]> {
  return page.evaluate(async () => {
    const names = await caches.keys()
    const lists = await Promise.all(
      names.map(async (n) => (await (await caches.open(n)).keys()).map((r) => r.url))
    )
    return lists.flat()
  })
}
