// K1: a review proposed while the person is in a scene must not drop keyboard focus to <body>
// (the dock's Send, Add files and Switch to voice are held while a review waits). SIMULATED
// API and voice (mockApi, fake ElevenLabs socket).
import type { Page } from "@playwright/test"

import { seedSession } from "../../src/session"
import { expect, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { capsule, k, readBack, stockRoutes } from "./task"
import { fakeAgent, v } from "./voice"

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi(readBack("120.00")))
})

/** On a scene with a text conversation going, keyboard focus on `target`, then a review arrives. */
async function reviewWhileOn(page: Page, target: (page: Page) => import("@playwright/test").Locator) {
  const agent = await fakeAgent(page)
  await stockRoutes(page)
  await page.goto("/scenes/stock")
  await page.getByLabel(v.message).fill("Set honey stock")
  await page.getByLabel(v.message).press("Enter")
  await expect(page.getByRole("button", { name: v.addFiles })).toBeVisible()
  await page.keyboard.press("Shift+Tab") // keyboard focus, so the browser shows it
  await target(page).focus()
  await expect(target(page)).toBeFocused()
  agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 120 }] },
  })
  await expect(capsule(page)).toHaveAttribute("data-task-state", "needs_approval")
  await page.waitForTimeout(200)
  expect(new URL(page.url()).pathname).toBe("/scenes/stock")
}
const onBody = (page: Page) => page.evaluate(() => document.activeElement === document.body)

for (const [name, target] of [
  ["Send", (p: Page) => p.getByRole("button", { name: v.send })],
  ["Add files", (p: Page) => p.getByRole("button", { name: v.addFiles })],
  ["Switch to voice", (p: Page) => p.getByRole("button", { name: "Switch to voice" })],
] as const)
  test(`K1 focus on ${name} when a review arrives in a scene: it moves to the review, never <body>`, async ({ page }) => {
    await reviewWhileOn(page, target)
    expect(await onBody(page), "focus lost to <body>").toBe(false)
    await expect(capsule(page).getByRole("button", { name: k.review })).toBeFocused()
  })

test("K1 typing in the message box when a review arrives: focus stays there", async ({ page }) => {
  await reviewWhileOn(page, (p) => p.getByLabel(v.message))
  await expect(page.getByLabel(v.message)).toBeFocused()
})
