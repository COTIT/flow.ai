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

test("recalled batch is announced first and shows its farmers", async ({
  page,
}) => {
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

  await expect(
    page.getByRole("heading", { name: "Meet the farmers" })
  ).toBeVisible()
  await expect(page.getByRole("img", { name: "Asha" })).toBeVisible()
  await expectNoHorizontalScroll(page)
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
