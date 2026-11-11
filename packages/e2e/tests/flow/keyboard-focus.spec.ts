// P9: keyboard and screen-reader focus. SIMULATED API + voice.
import type { Page } from "@playwright/test"

import { focusLook } from "../../src/focus"
import { seedSession } from "../../src/session"
import { mockApi } from "./api"
import { api as discoveryApi, d, OWNER } from "./discovery"
import { expect, test } from "./storefront-api"
import { proposeStock, readBack, receipt, scene, stockRoutes } from "./task"
import { fakeAgent, v } from "./voice"

const controls = (page: Page) => ({
  button: page.getByRole("button", { name: v.textInstead, exact: true }),
  field: page.getByLabel(v.message),
})

test.describe("P9-2: forced colours (Windows High Contrast)", () => {
  test.use({ forcedColors: "active" })
  test("focus is drawn as an outline on buttons and fields", async ({ page, api }) => {
    void api
    await page.goto("/")
    for (const [name, control] of Object.entries(controls(page))) {
      const look = await focusLook(control)
      expect(look.visible, name).toBe(true)
      expect(look.outline, `${name}: no outline in forced colours`).not.toBe("none")
      expect(look.width, name).toBeGreaterThanOrEqual(2)
    }
  })
})

test("P9-2: the normal focus look is unchanged (ring, no outline)", async ({ page, api }) => {
  void api
  await page.goto("/")
  const look = await focusLook(controls(page).button)
  expect(look.visible).toBe(true)
  expect(look.outline).toBe("none")
  expect(look.ring).not.toBe("none")
})

/** Presses Enter on `target` as a keyboard user would. */
async function pressByKeyboard(target: import("@playwright/test").Locator) {
  await target.page().keyboard.press("Tab")
  await target.focus()
  await target.page().keyboard.press("Enter")
}
const onBody = (page: Page) => page.evaluate(() => document.activeElement === document.body)

test("P9-1: dismissing the tip by keyboard moves focus to “What can Genesy do?”", async ({ page }) => {
  await seedSession(page)
  await mockApi(page, discoveryApi(OWNER))
  await page.goto("/")
  await pressByKeyboard(page.getByRole("button", { name: d.hint.dismiss }))
  await expect(page.getByRole("link", { name: d.hint.whatCanDo })).toBeFocused()
})

test.describe("P9-1: a decided review never drops focus to the page", () => {
  test.beforeEach(async ({ api }) => {
    const stock = readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
    api.on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => stock.body as object)
  })

  test("Approve by keyboard: focus follows to the receipt", async ({ page }) => {
    const agent = await fakeAgent(page)
    await stockRoutes(page)
    await proposeStock(page, agent)
    await pressByKeyboard(scene(page).getByRole("button", { name: v.approve }))
    await expect(receipt(page)).toHaveAttribute("data-receipt", "completed")
    await page.waitForTimeout(200)
    expect(await onBody(page), "focus lost to <body> after Approve").toBe(false)
  })

  test("Cancel by keyboard: focus moves to the conversation it returns to", async ({ page }) => {
    const agent = await fakeAgent(page)
    await stockRoutes(page)
    await proposeStock(page, agent)
    await pressByKeyboard(scene(page).getByRole("button", { name: v.cancel, exact: true }))
    await expect(page.getByRole("heading", { name: "Conversation" })).toBeFocused()
    // The cancel itself is read from the transcript's polite log.
    await expect(page.getByRole("log")).toContainText("Cancelled")
  })
})
