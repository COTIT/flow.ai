import type { Page } from "@playwright/test"

import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { mockApi } from "./api"
import {
  CLERK,
  LOW_STOCK,
  OWNER,
  api,
  d,
  label,
  pills,
} from "./discovery"

// P3-1: the explorer stays findable after the first-visit tip has gone (dismissed, or familiar
// after 5 uses), even when an owner's 4 suggestions are full. SAMPLE data, mock API.

test.beforeEach(async ({ page }) => {
  await seedSession(page)
})

const tip = (page: Page) =>
  page.getByRole("complementary", { name: d.hint.lead })
const small = (page: Page) =>
  page.getByRole("link", { name: d.hint.whatCanDo })

/** The small link is shown, 44 px tall, reached by keyboard, and opens the explorer. */
async function expectFindable(page: Page) {
  const link = small(page)
  await expect(link).toBeVisible()
  await expect(link).toHaveAttribute("href", "/explore")
  await expect(link).not.toHaveAttribute("tabindex", "-1")
  expect((await link.boundingBox())!.height).toBeGreaterThanOrEqual(44)
  await expectNoHorizontalScroll(page)
  await link.focus()
  await expect(link).toBeFocused()
  await page.keyboard.press("Enter")
  await expect(page).toHaveURL(/\/explore$/)
  await expect(
    page.getByRole("heading", { name: d.explorer.title })
  ).toBeVisible()
}

async function dismissTip(page: Page) {
  await expect(small(page)).toHaveCount(0) // no second entry beside the full tip
  await tip(page).getByRole("button", { name: d.hint.dismiss }).click()
  await expect(tip(page)).toHaveCount(0)
}

test("owner with alerts: suggestions full, the dismissed tip leaves a link to the explorer", async ({
  page,
}) => {
  await mockApi(page, api(OWNER, LOW_STOCK))
  await page.goto("/")
  // Full = the most it shows (P5: at most three).
  await expect(pills(page)).toHaveCount(3)
  await expect(pills(page).filter({ hasText: label("explore") })).toHaveCount(0)
  await dismissTip(page)
  await page.reload()
  await expect(tip(page)).toHaveCount(0)
  await expectFindable(page)
})

test("member: the dismissed tip leaves a link to the explorer", async ({
  page,
}) => {
  await mockApi(page, api(CLERK))
  await page.goto("/")
  await dismissTip(page)
  await expectFindable(page)
})

test("familiar after 5 uses: the tip becomes a link, never both", async ({
  page,
}) => {
  await mockApi(page, api(OWNER, LOW_STOCK))
  await page.goto("/")
  await expect(tip(page)).toBeVisible()
  await expect(small(page)).toHaveCount(0)
  // 5 uses on this device (Flow's activity store for u1 in w1), as if visited 5 times.
  await page.evaluate(() =>
    localStorage.setItem(
      "genesy.flow.activity:u1:w1",
      JSON.stringify({ uses: { explore: { n: 5, last: Date.now() } } })
    )
  )
  await page.reload()
  await expect(tip(page)).toHaveCount(0)
  await expectFindable(page)
})
