import { readdirSync, statSync } from "node:fs"
import { join } from "node:path"

import { expect, expectNoLayoutShift, test } from "../../src/test"

const FOLDER = join(import.meta.dirname, "../../../../apps/shop/public/afm")

test("AFM's content photos stay small: WebP only, under 1.5 MB together", () => {
  const files = readdirSync(FOLDER)
  expect(files.length).toBeGreaterThan(0)
  expect(files.filter((f) => !f.endsWith(".webp"))).toEqual([])
  const bytes = files.reduce((n, f) => n + statSync(join(FOLDER, f)).size, 0)
  expect(bytes).toBeLessThan(1.5 * 1024 * 1024)
})

for (const [path, count] of [
  ["/", 3],
  ["/stories", 4],
  ["/about", 1],
] as const)
  test(`${path}: AFM's photos load, are described in words, and don't shift the page`, async ({
    page,
  }) => {
    await page.goto(path)
    const photos = page.locator('main img[src^="/afm/"]')
    await expect(photos).toHaveCount(count)
    for (let i = 0; i < count; i++) {
      const img = photos.nth(i)
      const alt = (await img.getAttribute("alt")) ?? ""
      // A sentence, not a file name.
      expect(alt.length).toBeGreaterThan(20)
      expect(alt).not.toMatch(/\.(webp|png|jpe?g)$/i)
      await expect(img).toHaveAttribute("width", /^\d+$/)
      await expect(img).toHaveAttribute("height", /^\d+$/)
      await expect(img).toHaveCSS("object-fit", "contain")
      await img.scrollIntoViewIfNeeded()
      await expect
        .poll(() =>
          img.evaluate((el: HTMLImageElement) => el.complete && el.naturalWidth)
        )
        .toBeGreaterThan(0)
    }
    await page.evaluate(() => window.scrollTo(0, 0))
    await expectNoLayoutShift(page)
  })

test("/: the photos keep their elements once the page comes alive (no re-mount)", async ({
  page,
}) => {
  // Counts every AFM photo taken out of the page; a fading tile must not rebuild its contents.
  await page.addInitScript(() => {
    const w = window as unknown as { removedPhotos: number }
    w.removedPhotos = 0
    new MutationObserver((records) => {
      for (const r of records)
        for (const n of r.removedNodes)
          if (
            n instanceof Element &&
            (n.matches('img[src^="/afm/"]') ||
              n.querySelector('img[src^="/afm/"]'))
          )
            w.removedPhotos++
    }).observe(document, { childList: true, subtree: true })
  })
  await page.goto("/")
  await expect(page.locator("main [data-motion]").first()).toBeAttached()
  await page.waitForLoadState("networkidle")
  expect(
    await page.evaluate(
      () => (window as unknown as { removedPhotos: number }).removedPhotos
    )
  ).toBe(0)
})

test("the stories page keeps initials for someone without a photo", async ({
  page,
}) => {
  await page.goto("/stories")
  const rachel = page
    .getByRole("listitem")
    .filter({ has: page.getByRole("heading", { name: "Rachel Bosire" }) })
  await expect(rachel.locator("img")).toHaveCount(0)
  await expect(rachel.getByText("RB", { exact: true })).toBeAttached()
})

test("a story photo that fails to decode falls back to initials", async ({
  page,
}) => {
  await page.route("**/afm/benard-omondi.webp", (route) =>
    route.fulfill({ contentType: "image/webp", body: "Invalid image" })
  )
  await page.goto("/stories")
  const benard = page.getByRole("listitem").filter({
    has: page.getByRole("heading", { name: /Benard Omondi/ }),
  })
  await benard.scrollIntoViewIfNeeded()
  await expect(benard.getByText("BO", { exact: true })).toBeVisible()
  await expect(benard.locator("img")).toHaveCount(0)
})
