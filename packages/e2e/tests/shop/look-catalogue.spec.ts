import type { Locator } from "@playwright/test"

import {
  expect,
  expectNoHorizontalScroll,
  expectNoLayoutShift,
  expectNoMotion,
  expectTapTargets,
  test,
} from "../../src/test"

const HEADLINE = "Rooted in African soil. Shared with the world."

const css = (l: Locator, prop: string) =>
  l.evaluate((el, p) => getComputedStyle(el).getPropertyValue(p), prop)

test("home leads with the mission, the trace box and AFM's categories", async ({
  page,
  isMobile,
}) => {
  await page.goto("/")
  await expect(page.locator("body")).toHaveClass(/\btheme-glass\b/)
  const header = page.getByRole("banner")
  expect(await css(header.locator(".."), "max-width")).toBe("1200px")
  await expect(header.getByRole("link", { name: "Cart, empty" })).toBeVisible()

  await expect(page.getByRole("heading", { level: 1 })).toHaveAccessibleName(
    HEADLINE
  )
  await expect(page.getByLabel("Batch code")).toBeVisible()
  // What Genesy adds, said plainly: passport, live stock, farmers per batch.
  const promises = page.getByRole("region", {
    name: "What you can check yourself",
  })
  for (const title of [
    "A passport for every batch",
    "Live stock",
    "Farmers named per batch",
  ])
    await expect(promises.getByRole("heading", { name: title })).toBeVisible()

  const categories = page.getByRole("region", { name: "Shop by category" })
  await expect(categories.getByRole("link")).toHaveCount(10)
  await expect(
    categories.getByRole("link", { name: /^Honey 4 products/ })
  ).toBeVisible()
  const columns = (
    await css(categories.getByRole("list"), "grid-template-columns")
  ).split(" ")
  expect(columns).toHaveLength(isMobile ? 2 : 5)

  // In stock now: sold-out products stay off the home page.
  const featured = page.getByRole("region", { name: "In stock now" })
  await expect(featured.getByRole("listitem")).toHaveCount(8)
  await expect(featured.getByText("Sold out")).toHaveCount(0)

  const people = page.getByRole("region", { name: "The people behind it" })
  await expect(
    people.getByRole("heading", { name: "The Illundo bee farmers" })
  ).toBeVisible()

  const footer = page.getByRole("contentinfo")
  await expect(footer).toContainText(HEADLINE)
  // AFM's shop under AFM's name; Genesy is credited for the tracing.
  await expect(
    header.getByRole("link", { name: "African Farmers Market home" })
  ).toContainText("African Farmers Market")
  await expect(footer).toContainText("Batches traced with Genesy")
  await expect(footer.getByLabel("Language")).toBeVisible()
  await expect(header.getByLabel("Language")).toBeVisible({
    visible: !isMobile,
  })

  await expectNoHorizontalScroll(page)
  if (isMobile) await expectTapTargets(page)
  await expectNoLayoutShift(page)
})

test("the shop grid fits four across on desktop and two on a phone", async ({
  page,
  isMobile,
}) => {
  await page.goto("/shop")
  const grid = page.getByRole("list", { name: "All products" })
  const columns = (await css(grid, "grid-template-columns")).split(" ")
  expect(columns).toHaveLength(isMobile ? 2 : 4)

  const honey = page.getByRole("link", { name: /Ilundo Wild Forest Honey/ })
  await expect(honey).toContainText("$14.99")
  // The card's small tag is the category, not the brand.
  await expect(honey).toContainText("Honey")
  await expect(honey.locator('[data-slot="badge"]')).toHaveText("Few left")
  await expect(honey.locator('[data-slot="badge"]')).toHaveAttribute(
    "data-variant",
    "gold"
  )
  await expect(honey.locator('img[alt=""]')).toBeVisible()
  await expect(page.getByRole("link", { name: /Beeswax Block/ })).toContainText(
    "Sold out"
  )
  await expectNoHorizontalScroll(page)
  if (isMobile) await expectTapTargets(page)
  await expectNoLayoutShift(page)
})

test("the server sends the headline, categories and products (search engines see them)", async ({
  request,
}) => {
  const html = await (await request.get("/")).text()
  expect(html).toContain("Rooted in African soil.")
  expect(html).toContain("Figurines &amp; Games")
  expect(html).toContain("Mary&#x27;s Tanzanian Wildflower Honey")
})

test("with motion, the headline comes into focus; photos glare only under a mouse", async ({
  page,
  isMobile,
}) => {
  await page.goto("/")
  const h1 = page.getByRole("heading", { level: 1 })
  await expect(h1.locator("[data-motion]")).toHaveCount(1)
  await expect(h1).toHaveAccessibleName(HEADLINE)
  const honey = page.getByRole("link", { name: /Ilundo Wild Forest Honey/ })
  await expect(honey.locator("[data-motion]")).toHaveCount(isMobile ? 0 : 1)
  await expectNoLayoutShift(page)
})

test.describe("with reduced motion", () => {
  test.use({ reducedMotion: "reduce" })

  test("nothing animates and the words stay as written", async ({ page }) => {
    await page.goto("/")
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(HEADLINE)
    await expect(
      page.getByRole("link", { name: /Ilundo Wild Forest Honey/ })
    ).toBeVisible()
    await expectNoMotion(page)
  })
})
