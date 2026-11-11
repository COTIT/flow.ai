import type { Page } from "@playwright/test"

import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { fakeAgent, v } from "./voice"

// SIMULATED voice: mock upstream behind Flow's real proxy + a fake ElevenLabs socket.

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi())
})

/** The conversation fits the screen: its controls are on screen without scrolling the page. */
async function expectFitsScreen(page: Page) {
  await expectNoHorizontalScroll(page)
  const { scroll, height } = await page.evaluate(() => ({
    scroll: document.documentElement.scrollHeight,
    height: window.innerHeight,
  }))
  expect(scroll, "page scrolls down").toBeLessThanOrEqual(height)
}

test("the whole conversation works from the keyboard", async ({ page }) => {
  await fakeAgent(page, (text, a) => a.say(`Answer: ${text}`))
  await page.goto("/")
  const start = page.getByRole("button", { name: v.textInstead, exact: true })
  await expect(start).toBeVisible()
  for (
    let i = 0;
    i < 30 && !(await start.evaluate((b) => b === document.activeElement));
    i++
  )
    await page.keyboard.press("Tab")
  await expect(start).toBeFocused()
  await page.keyboard.press("Enter")
  const box = page.getByLabel(v.message)
  await expect(box).toBeVisible()
  await box.focus()
  await page.keyboard.type("Typed with keys")
  await page.keyboard.press("Enter")
  await expect(page.getByRole("log")).toContainText("Answer: Typed with keys")
  await expectFitsScreen(page)
})

// Tablet portrait and landscape share the one shell layout (no results pane or sheet any more).
for (const size of [
  { name: "tablet portrait", width: 834, height: 1194 },
  { name: "tablet landscape", width: 1024, height: 768 },
]) {
  test.describe(size.name, () => {
    test.use({ viewport: { width: size.width, height: size.height } })

    test("home, the orb and the dock are on screen, nothing clipped", async ({
      page,
    }) => {
      await page.goto("/")
      await expect(
        page.getByRole("button", { name: v.textInstead, exact: true })
      ).toBeInViewport()
      await expect(page.locator("[data-orb]")).toBeInViewport()
      await expect(page.getByLabel(v.message)).toBeInViewport()
      await expectFitsScreen(page)
    })
  })
}
