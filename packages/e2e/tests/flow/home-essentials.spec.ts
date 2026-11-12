// P6: the start screen keeps the essentials in sight and stays short. Who and which business,
// the shop one tap away, the microphone note and the tip only while they help, and the shop's
// versions in plain words. SAMPLE data (fake API world), SIMULATED voice.
import type { Locator, Page } from "@playwright/test"

import { seedSession } from "../../src/session"
import { expectNoHorizontalScroll, test as single } from "../../src/test"
import { flowApi, ILUNDO, mockApi } from "./api"
import { menuItem, moreMenu } from "./scenes-api"
import { designs, expect, test } from "./storefront-api"
import { fakeAgent, v } from "./voice"

const business = (page: Page) => page.getByRole("group", { name: "Your business" })
const micNote = (page: Page) => page.getByText(v.micHint)
const tip = (page: Page) => page.getByRole("complementary", { name: "Not sure where to begin?" })

/** Two controls on one calm line: their middles at the same height, nothing scrolls sideways. */
async function oneLine(a: Locator, b: Locator) {
  const [x, y] = [(await a.boundingBox())!, (await b.boundingBox())!]
  expect(Math.abs(x.y + x.height / 2 - (y.y + y.height / 2))).toBeLessThan(4)
}

test.describe("two businesses", () => {
  test.beforeEach(({ api }) => void api)

  test("the greeting names the person; the business switches and the shop opens from one line", async ({ page }) => {
    await page.goto("/")
    await expect(page.getByText(/^Good (morning|afternoon|evening), Amina\.$/)).toBeVisible()
    const line = business(page)
    const pick = line.getByRole("combobox", { name: "Switch business" })
    const shop = line.getByRole("link", { name: "Your shop" })
    await expect(pick).toContainText(ILUNDO.display_name)
    await oneLine(pick, shop)
    // No more than the two: the line stays calm (keep-simple rule).
    await expect(line.getByRole("button")).toHaveCount(0)
    await expect(line.getByRole("link")).toHaveCount(1)
    await expectNoHorizontalScroll(page)
    await pick.click()
    await page.getByRole("option", { name: "Ruaha Growers", exact: true }).click()
    await expect(pick).toContainText("Ruaha Growers")
    await shop.click()
    await expect(page).toHaveURL(/\/storefront$/)
  })

  test("the microphone note and the tip show only until the person has started", async ({ page }) => {
    await fakeAgent(page)
    await page.goto("/")
    await expect(micNote(page)).toBeVisible()
    await expect(tip(page)).toBeVisible()
    // First conversation (typed): afterwards the note has done its job.
    await page.getByRole("button", { name: "Type instead", exact: true }).click()
    await expect(page.getByRole("button", { name: v.addFiles })).toBeVisible()
    await page.getByRole("button", { name: v.end }).first().click()
    // A scene opened counts as having started: the tip becomes the quiet link.
    await page.goto("/scenes/stock")
    await expect(page.getByRole("heading", { level: 1, name: "Stock Watch" })).toBeVisible()
    await page.goto("/")
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
    await expect(micNote(page)).toHaveCount(0)
    await expect(tip(page)).toHaveCount(0)
    await expect(page.getByRole("link", { name: "What can Genesy do?" })).toBeVisible()
  })

  test("hidden scenes and reset are one small line of links, not buttons", async ({ page }) => {
    await page.goto("/scenes/sales")
    await moreMenu(page).click()
    await menuItem(page, "Remove from Your Flow").click()
    await expect(page).toHaveURL(/\/$/)
    const hidden = page.getByText(/^Hidden scenes/)
    const reset = page.getByRole("button", { name: "Reset Your Flow" })
    await expect(hidden).toBeVisible()
    await expect(reset).toBeVisible()
    await oneLine(hidden, reset)
    for (const el of [hidden, reset]) {
      const look = await el.evaluate((e) => {
        const s = getComputedStyle(e)
        return { border: s.borderTopWidth, size: parseFloat(s.fontSize), fill: s.backgroundColor }
      })
      expect(look.border, "no button outline").toBe("0px")
      expect(look.size, "small text").toBeLessThanOrEqual(14)
      expect(look.fill, "no button fill").toMatch(/rgba\(0, 0, 0, 0\)|transparent/)
    }
  })

  test("at 200% zoom a start-screen control that takes keyboard focus is in view", async ({ page }) => {
    await page.setViewportSize({ width: 640, height: 400 })
    await page.goto("/")
    const target = page.getByRole("link", { name: "See what needs attention" })
    await expect(target).toBeAttached()
    for (let i = 0; i < 40 && !(await target.evaluate((el) => el === document.activeElement)); i++)
      await page.keyboard.press("Tab")
    await expect(target).toBeFocused()
    await expect(target).toBeInViewport()
  })

  test("the shop's versions read plainly: live now, new look ready, no codes", async ({ page, api }) => {
    designs(api)
    await page.goto("/storefront")
    const main = page.locator("main")
    await expect(main.getByText("Live now")).toBeVisible()
    await expect(main.getByText("New look ready")).toBeVisible()
    await expect(main.getByText(/^Version 1\b/)).toBeVisible()
    await expect(main.getByText(/^Version 2\b/)).toBeVisible()
    await expect(page.getByRole("button", { name: /^Preview version 2, saved / })).toBeVisible()
    await expect(main).not.toContainText(/revision|\b0{4}\d{4}\b/i)
    await expectNoHorizontalScroll(page)
  })
})

single("one business: its name in the line, nothing to switch", async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi())
  await page.goto("/")
  const line = business(page)
  await expect(line).toContainText(ILUNDO.display_name)
  await expect(line.getByRole("combobox")).toHaveCount(0)
  await expect(line.getByRole("link", { name: "Your shop" })).toBeVisible()
})
