import type { Page } from "@playwright/test"

import { LOT } from "../../mock/fixtures.mjs"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"

test("released batch shows its passport", async ({ page }) => {
  const response = await page.goto(`/b/${LOT.released}`)
  expect(response?.status()).toBe(200)

  await expect(
    page.getByRole("heading", {
      level: 1,
      name: "Wild forest honey, harvest July 2026",
    })
  ).toBeVisible()
  await expect(page.getByText(/has been recalled/)).toHaveCount(0)
  for (const section of [
    "Where it comes from",
    "The story",
    "What's inside",
    "Certificates",
  ])
    await expect(
      page.getByRole("heading", { level: 2, name: section })
    ).toBeVisible()
  // No farmer has consented yet: the section is left out, not shown empty.
  await expect(
    page.getByRole("heading", { name: "Meet the farmers" })
  ).toHaveCount(0)

  const map = page.getByRole("link", { name: "View the area on OpenStreetMap" })
  await expect(map).toHaveAttribute(
    "href",
    "https://www.openstreetmap.org/#map=11/-7.77/35.69"
  )
  await expect(map).toHaveAttribute("target", "_blank")
  await expect(map).toHaveAttribute("rel", "noopener noreferrer")

  // Signed images load from the file host (CSP img-src lets exactly that origin in).
  const banner = page.locator("header img").first()
  await expect(banner).toBeVisible()
  expect(
    await banner.evaluate((img: HTMLImageElement) => img.naturalWidth)
  ).toBeGreaterThan(0)
  await expectNoHorizontalScroll(page)
})

test("recalled batch is announced first", async ({ page }) => {
  await page.goto(`/b/${LOT.recalled}`)
  // Next's route announcer is also role=alert (empty); pick the banner by its text.
  const alert = page
    .getByRole("alert")
    .filter({ hasText: "This batch has been recalled" })
  await expect(alert).toBeVisible()
  // The banner comes before the title in reading order.
  const order = await page.evaluate(() => {
    const alert = document.querySelector(
      '[role="alert"]:not(#__next-route-announcer__)'
    )!
    const title = document.querySelector("h1")!
    return (
      alert.compareDocumentPosition(title) & Node.DOCUMENT_POSITION_FOLLOWING
    )
  })
  expect(order).toBeTruthy()
  await expectNoHorizontalScroll(page)
})

test.describe("farmer groups", () => {
  // The village name alone on its line (materials mention places too).
  const village = (page: Page, name: string) =>
    page
      .getByRole("listitem")
      .filter({ has: page.getByText(name, { exact: true }) })

  test("show a village's farmer count and harvest window", async ({ page }) => {
    await page.goto(`/b/${LOT.recalled}`)
    await expect(
      page.getByRole("heading", { name: "Meet the farmers" })
    ).toBeVisible()
    const ilundo = village(page, "Ilundo")
    await expect(ilundo).toContainText("12 farmers")
    await expect(ilundo).toContainText("Harvested July 1, 2026 – July 20, 2026")
    // Groups only: no farmer photos (or any image) in the section.
    const section = page
      .locator("section")
      .filter({ has: page.getByRole("heading", { name: "Meet the farmers" }) })
    await expect(section.locator("img")).toHaveCount(0)
    await expectNoHorizontalScroll(page)
  })

  test("leave out the count for fewer than 3 farmers", async ({ page }) => {
    await page.goto(`/b/${LOT.recalled}`)
    const mtera = village(page, "Mtera")
    await expect(mtera).toContainText(
      "Harvested August 2, 2026 – August 9, 2026"
    )
    await expect(mtera).not.toContainText(/farmers|null/)
  })
})

test.describe("codes that open no passport", () => {
  test.use({
    allow: {
      responses: [
        { url: `/b/${LOT.unknown}`, status: 404 },
        { url: "/b/abc", status: 404 },
      ],
    },
  })

  test("an unknown code is a real 404", async ({ page }) => {
    const response = await page.goto(`/b/${LOT.unknown}`)
    expect(response?.status()).toBe(404)
    await expect(
      page.getByRole("heading", { name: "Page not found" })
    ).toBeVisible()
  })

  test("a malformed code is a 404 without calling the API", async ({
    page,
  }) => {
    const response = await page.goto("/b/abc")
    expect(response?.status()).toBe(404)
  })
})
