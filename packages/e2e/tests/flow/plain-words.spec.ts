// P6 plain words: the screens an older, non-technical owner meets say what happens in everyday
// words. SIMULATED voice (fake ElevenLabs socket + Office-shaped answers) over the scene world.
import type { Page } from "@playwright/test"

import { expect, test } from "./storefront-api"
import { stockRoutes } from "./task"
import { fakeAgent, v } from "./voice"
import discoveryEn from "../../../../apps/flow/messages/en/discovery.json" with { type: "json" }
import discoveryNl from "../../../../apps/flow/messages/nl/discovery.json" with { type: "json" }
import discoverySw from "../../../../apps/flow/messages/sw/discovery.json" with { type: "json" }

test.beforeEach(({ api }) => void api)

/** System words the review found confusing; none may show on these screens. */
const JARGON =
  /inspection|Request failed|Executing|priorit|fulfillment|Review action|Approve action|Voice session|Use text instead|Snooze|Acknowledge|Flow noticed|My Routines/i

async function noJargon(page: Page) {
  await expect(page.locator("body")).not.toContainText(JARGON)
}

test("a review asks plainly: Check this change, Yes do it, No cancel, Or say", async ({ page }) => {
  const agent = await fakeAgent(page)
  await stockRoutes(page)
  await page.goto("/")
  await page.getByRole("button", { name: "Type instead", exact: true }).click()
  await expect(page.getByRole("button", { name: v.addFiles })).toBeVisible()
  agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 120 }] },
  })
  const review = page.locator("#flow-review-title")
  await expect(review).toContainText("Check this change")
  await expect(page.getByRole("button", { name: "Yes, do it", exact: true })).toBeVisible()
  await expect(page.getByRole("button", { name: "No, cancel", exact: true })).toBeVisible()
  await expect(page.getByText("Or say:", { exact: false })).toBeVisible()
  await expect(page.getByRole("status", { name: "Active task" })).toContainText("Working")
  await noJargon(page)
})

test("a file says Ready to send, then Sent to Genesy", async ({ page }) => {
  await fakeAgent(page, (_, a) => setTimeout(() => a.say("Got it."), 50), { files: true })
  await page.goto("/")
  await page.getByRole("button", { name: "Type instead", exact: true }).click()
  await page.getByLabel(v.chooseFiles).setInputFiles({
    name: "note.pdf",
    mimeType: "application/pdf",
    buffer: Buffer.from("%PDF-1.4\n%%EOF\n"),
  })
  await expect(page.getByText("Ready to send")).toBeVisible()
  await page.getByLabel(v.message).fill("Here is the delivery note")
  await page.getByRole("button", { name: v.send, exact: true }).click()
  await expect(page.getByText("Sent to Genesy")).toBeVisible()
  await noJargon(page)
})

test("what needs attention reads plainly on home and in its scene", async ({ page }) => {
  await page.goto("/")
  const card = page.getByRole("region", { name: "Needs your attention" })
  await expect(card).toContainText("things need your attention")
  await expect(card).toContainText("orders waiting to be sent")
  await card.getByRole("link", { name: "See what needs attention" }).click()
  await expect(page.getByRole("heading", { level: 1, name: "Needs your attention" })).toBeVisible()
  // The row's other choices wait in Later ▾ (P3) or, at tablet width, one ⋯ menu.
  await expect(page.getByRole("button", { name: /^(Mark as seen|Actions for .*|Later .*)$/ }).first()).toBeVisible()
  const menu = page.getByRole("button", { name: /^(Later|Actions for) / }).first()
  const actions = (await menu.isVisible()) ? (await menu.click(), page.getByRole("menu")) : page.locator("main")
  await expect(actions.getByText("Mark as seen").first()).toBeVisible()
  await expect(actions.getByText(/in a week/i).first()).toBeVisible()
  await page.keyboard.press("Escape")
  await noJargon(page)
})

test("a call that ends says so plainly, and how to start again", async ({ page }) => {
  await fakeAgent(page)
  await page.goto("/scenes/routines")
  await expect(page.getByRole("heading", { level: 1, name: "Scheduled reports" })).toBeVisible()
  await page.getByLabel(v.message).fill("Hello")
  await page.getByRole("button", { name: v.send, exact: true }).click()
  await page.getByRole("button", { name: "End conversation" }).first().click()
  await expect(page.getByText("Call ended.")).toBeVisible()
  await expect(page.getByText("Tap the orb to talk again, or type below.")).toBeVisible()
  await noJargon(page)
})

for (const [locale, groups] of [
  ["en", discoveryEn.discovery.explorer.groups],
  ["nl", discoveryNl.discovery.explorer.groups],
  ["sw", discoverySw.discovery.explorer.groups],
] as const)
  test(`${locale}: the explorer's four kinds of task fit side by side, none hidden or overlapping`, async ({ page, context, baseURL }) => {
    await context.addCookies([{ name: "NEXT_LOCALE", value: locale, url: baseURL! }])
    await page.goto("/explore")
    const tabs = page.getByRole("tab")
    await expect(tabs).toHaveText([groups.understand, groups.operate, groups.automate, groups.create])
    const boxes = await tabs.evaluateAll((els) =>
      els.map((e) => ({ left: e.getBoundingClientRect().left, right: e.getBoundingClientRect().right, text: e.scrollWidth, width: e.clientWidth }))
    )
    for (const [i, b] of boxes.entries()) {
      expect(b.text, `tab ${i} text fits its tab`).toBeLessThanOrEqual(b.width + 1)
      if (i) expect(b.left, `tab ${i} starts after the one before`).toBeGreaterThanOrEqual(boxes[i - 1]!.right - 1)
    }
    const list = await page.getByRole("tablist").evaluate((e) => ({ scroll: e.scrollWidth, width: e.clientWidth }))
    expect(list.scroll, "all four in view, no sideways scroll").toBeLessThanOrEqual(list.width + 4)
    await page.getByRole("tab", { name: groups.operate }).click()
    await expect(page.locator('[data-capability="adjust_stock"]')).toBeVisible()
  })
