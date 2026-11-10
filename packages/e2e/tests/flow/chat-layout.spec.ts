import type { Page } from "@playwright/test"

import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { chat, fakeAgent, v } from "./voice"

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

for (const size of [
  { name: "tablet portrait", width: 834, height: 1194, panes: 1 },
  { name: "tablet landscape", width: 1024, height: 768, panes: 2 },
]) {
  test.describe(size.name, () => {
    test.use({ viewport: { width: size.width, height: size.height } })

    test(`shows ${size.panes === 2 ? "two panes" : "one column and a results sheet"}, nothing clipped`, async ({
      page,
    }) => {
      await page.goto("/")
      await expect(
        page.getByRole("button", { name: v.textInstead, exact: true })
      ).toBeInViewport()
      if (size.panes === 2) {
        await expect(
          page.getByRole("region", { name: chat.results })
        ).toBeVisible()
      } else {
        await expect(
          page.getByRole("region", { name: chat.results })
        ).toHaveCount(0)
        await page
          .getByRole("button", { name: new RegExp(`^${chat.results}`) })
          .click()
        await expect(
          page.getByRole("dialog", { name: chat.results })
        ).toBeVisible()
        await page.getByRole("button", { name: chat.closeResults }).click()
      }
      await expectFitsScreen(page)
    })
  })
}
