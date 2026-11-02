import type { Page } from "@playwright/test"

import { expect, expectNoHorizontalScroll, test } from "../../src/test"

const STEPS = [/^Hive/, /^Harvest/, /^Passport/, /^Scan/, /^Sold/, /^Paid back/]

/** Scrolls so the pinned journey is at `progress` (0 = its start, 1 = its end). */
async function scrollJourney(page: Page, progress: number) {
  await page.evaluate((f) => {
    const j = document.querySelector<HTMLElement>("[data-journey]")!
    const top = j.getBoundingClientRect().top + scrollY
    scrollTo(0, top + f * (j.offsetHeight - innerHeight))
  }, progress)
}

test("all six steps of the journey are there as text", async ({ page }) => {
  await page.goto("/")
  await expect(
    page.getByRole("heading", {
      level: 2,
      name: "Watch one jar travel — and the money travel back.",
    })
  ).toBeAttached()
  await expect(page.locator("#journey [data-step]")).toHaveText(STEPS)
})

test("scrolling plays the journey from the hive to the passbook", async ({
  page,
}) => {
  await page.goto("/")
  await expect(page.locator(".landing")).toHaveClass(/\bmotion\b/)
  await scrollJourney(page, 0.2)
  await expect(page.locator('#journey [data-step="1"]')).toHaveAttribute(
    "aria-current",
    "step"
  )
  await expect(page.locator('#journey [data-step="1"]')).toBeVisible()
  await scrollJourney(page, 0.99)
  await expect(page.locator('#journey [data-step="6"]')).toHaveAttribute(
    "aria-current",
    "step"
  )
  await expect(page.locator('#journey [data-step="1"]')).not.toHaveAttribute(
    "aria-current"
  )
  // The coin is home: back at the farmer's passbook.
  await expect(page.locator("[data-coin]")).toHaveCSS("opacity", "1")
  await expectNoHorizontalScroll(page)
})

test.describe("with reduced motion", () => {
  test.use({ reducedMotion: "reduce" })

  test("the journey is still and complete", async ({ page }) => {
    await page.goto("/")
    await expect(page.locator(".landing")).not.toHaveClass(/\bmotion\b/)
    await expect(page.locator("#journey .j-pin")).toHaveCSS(
      "position",
      "static"
    )
    for (const step of await page.locator("#journey [data-step]").all())
      await expect(step).toBeVisible()
    await expect(page.locator("#journey .j-part").first()).toHaveCSS(
      "opacity",
      "1"
    )
    await expectNoHorizontalScroll(page)
  })
})

test("dark mode keeps the journey readable", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark" })
  await page.goto("/")
  await expect(page.locator("html")).toHaveClass(/\bdark\b/)
  await expect(page.locator("#journey [data-step]")).toHaveText(STEPS)
})
