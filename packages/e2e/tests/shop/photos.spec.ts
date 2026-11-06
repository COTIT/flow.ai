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
  ["/", 4],
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
