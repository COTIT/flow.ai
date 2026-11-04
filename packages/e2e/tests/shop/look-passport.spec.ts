import type { Locator, Page } from "@playwright/test"

import { LOT } from "../../mock/fixtures.mjs"
import {
  expect,
  expectNoHorizontalScroll,
  expectNoLayoutShift,
  expectNoMotion,
  expectTapTargets,
  test,
} from "../../src/test"

// Mocks `ShopPassport`, `ShopPassportRecalled` and `ShopStates` in docs/design/web-redesign/canvas.
const TITLE = "Wild forest honey, harvest July 2026"

const css = (l: Locator, prop: string) =>
  l.evaluate((el, p) => getComputedStyle(el).getPropertyValue(p), prop)

const tile = (page: Page, heading: string) =>
  page
    .locator("main section")
    .filter({ has: page.getByRole("heading", { level: 2, name: heading }) })

/** Is `first` before `second` in reading order? */
const before = async (first: Locator, second: Locator) =>
  first.evaluate(
    (a, b) =>
      Boolean(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING),
    await second.elementHandle()
  )

test("a released passport is the bento from the mockup", async ({
  page,
  isMobile,
}) => {
  await page.goto(`/b/${LOT.released}`)

  const hero = page.locator("main header")
  const h1 = hero.getByRole("heading", { level: 1, name: TITLE })
  expect(await css(h1, "font-size")).toBe("32px")
  await expect(hero).toContainText(
    "Ilundo Wild Forest Honey · Batch 7K3M9QX2WD"
  )
  await expect(hero.getByText("Released", { exact: true })).toBeVisible()
  await expect(hero).toContainText(
    "Harvested July 14, 2026 · Best before July 14, 2028"
  )
  expect(await css(hero.locator("img").first(), "height")).toBe("280px")

  const where = tile(page, "Where it comes from")
  await expect(where).toContainText("Ilundo miombo forest")
  const inside = tile(page, "What's inside")
  await expect(inside).toContainText("Raw honey")
  await expect(inside).toContainText("100%")
  // Facts blocks from the story are tiles here.
  await expect(inside).toContainText("Hives")
  await expect(inside).toContainText("312")
  const story = tile(page, "The story")
  await expect(story).toContainText("Log hives hang high in the miombo trees.")
  await expect(
    story.getByRole("img", { name: "Hives above Ilundo" })
  ).toBeVisible()
  await expect(
    page.getByText("The bees know the forest better than we do.")
  ).toBeVisible()
  const certificates = tile(page, "Certificates")
  await expect(
    certificates.getByRole("link", { name: "View certificate" })
  ).toBeVisible()
  // No farmer has consented yet: the tile is left out, not shown empty.
  await expect(
    page.getByRole("heading", { name: "Meet the farmers" })
  ).toHaveCount(0)

  // Pairs side by side on a desktop (7 + 5 columns), one under the other on a phone.
  // Retried: the motion island swaps a tile for its client copy, which has no box for a moment.
  await expect(async () => {
    const whereBox = await where.boundingBox()
    const insideBox = await inside.boundingBox()
    expect(whereBox && insideBox, "both tiles laid out").toBeTruthy()
    if (isMobile)
      expect(insideBox!.y).toBeGreaterThan(whereBox!.y + whereBox!.height)
    else {
      expect(insideBox!.x).toBeGreaterThan(whereBox!.x + whereBox!.width)
      expect(whereBox!.width).toBeGreaterThan(insideBox!.width)
    }
  }).toPass()

  await expectNoHorizontalScroll(page)
  if (isMobile) await expectTapTargets(page)
  await expectNoLayoutShift(page)
})

test("the main content shows within 2.5 s on a phone", async ({
  page,
  isMobile,
}) => {
  test.skip(!isMobile, "measured at 375 px")
  await page.goto(`/b/${LOT.released}`)
  await expect(
    page.getByRole("heading", { level: 1, name: TITLE })
  ).toBeVisible()
  const lcp = await page.evaluate(
    () =>
      new Promise<number>((resolve) => {
        new PerformanceObserver((list) => {
          const entries = list.getEntries()
          resolve(entries.at(-1)?.startTime ?? Infinity)
        }).observe({ type: "largest-contentful-paint", buffered: true })
      })
  )
  expect(lcp, "largest contentful paint (ms)").toBeLessThan(2500)
})

test("a recalled batch: the red banner comes first, the badge says so", async ({
  page,
}) => {
  await page.goto(`/b/${LOT.recalled}`)
  const banner = page
    .getByRole("alert")
    .filter({ hasText: "This batch has been recalled" })
  await expect(banner).toBeVisible()
  expect(await css(banner, "background-color")).toBe("rgb(180, 35, 24)")
  const h1 = page.getByRole("heading", { level: 1 })
  expect(await before(banner, h1)).toBe(true)
  await expect(
    page.locator("main header").getByText("Recalled", { exact: true })
  ).toBeVisible()
  await expect(
    page.getByRole("heading", { level: 2, name: "Meet the farmers" })
  ).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test("a batch on hold: a warning in the banner's place", async ({ page }) => {
  await page.goto(`/b/${LOT.quarantine}`)
  const warning = page
    .locator('[data-variant="warning"]')
    .filter({ hasText: "This batch is on hold while it is being checked." })
  await expect(warning).toBeVisible()
  expect(await before(warning, page.getByRole("heading", { level: 1 }))).toBe(
    true
  )
  await expect(
    page.locator("main header").getByText("On hold", { exact: true })
  ).toBeVisible()
})

test("with motion, tiles below the fold fade in; the top of the page never waits", async ({
  page,
}) => {
  await page.goto(`/b/${LOT.released}`)
  await expect(page.locator("main [data-motion]").first()).toBeAttached()
  // The hero is drawn at once (it holds the largest paint).
  await expect(page.locator("main header")).toHaveCSS("opacity", "1")
  // Scrolled into view, every tile ends fully shown.
  await page
    .getByRole("heading", { name: "Certificates" })
    .scrollIntoViewIfNeeded()
  await expect(tile(page, "Certificates")).toHaveCSS("opacity", "1")
  await expectNoLayoutShift(page)
})

test.describe("with reduced motion", () => {
  test.use({ reducedMotion: "reduce" })

  test("nothing animates", async ({ page }) => {
    await page.goto(`/b/${LOT.released}`)
    await expect(
      page.getByRole("heading", { level: 1, name: TITLE })
    ).toBeVisible()
    await expectNoMotion(page)
  })
})

test.describe("not found", () => {
  test.use({
    allow: {
      responses: [
        { url: `/b/${LOT.unknown}`, status: 404 },
        { url: "/no-such-page", status: 404 },
      ],
    },
  })

  test("an unknown batch is the empty state in a card, with the QR icon", async ({
    page,
  }) => {
    await page.goto(`/b/${LOT.unknown}`)
    const empty = page.locator('[data-slot="empty-state"]')
    await expect(
      empty.getByRole("heading", { level: 1, name: "We can't find this batch" })
    ).toBeVisible()
    await expect(
      empty.getByRole("link", { name: "Browse the shop" })
    ).toBeVisible()
    await expectNoHorizontalScroll(page)
  })

  test("an unknown page is the empty state in a card", async ({ page }) => {
    await page.goto("/no-such-page")
    const empty = page.locator('[data-slot="empty-state"]')
    await expect(
      empty.getByRole("heading", { level: 1, name: "Page not found" })
    ).toBeVisible()
    await expect(
      empty.getByRole("link", { name: "Browse the shop" })
    ).toBeVisible()
  })
})
