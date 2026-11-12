// P9: keyboard and screen-reader focus. SIMULATED API + voice.
import type { Page } from "@playwright/test"

import { focusLook } from "../../src/focus"
import { seedSession } from "../../src/session"
import { mockApi } from "./api"
import { sc } from "./scenes-api"
import { s } from "./shell"
import { api as discoveryApi, d, OWNER } from "./discovery"
import { expect, test } from "./storefront-api"
import { proposeStock, readBack, receipt, scene, stockRoutes } from "./task"
import { fakeAgent, startText, v } from "./voice"

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

test.describe("P9-4: a review or a voice upgrade never strands keyboard focus", () => {
  test.beforeEach(async ({ api }) => {
    const stock = readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
    api.on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => stock.body as object)
  })

  test("P9-4a: a review arriving while the person types keeps the composer, is announced and is reachable by Tab", async ({
    page,
  }) => {
    const agent = await fakeAgent(page)
    await stockRoutes(page)
    await page.goto("/")
    await startText(page)
    const box = page.getByLabel(v.message)
    await box.focus()
    await page.keyboard.type("set honey to")
    agent.call("stock_set_levels", {
      path: { product: "p-500" },
      body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 120 }] },
    })
    const approve = scene(page).getByRole("button", { name: v.approve })
    await expect(approve).toBeVisible()
    await page.waitForTimeout(300)
    // Never taken from someone typing; what they typed stays.
    await expect(box).toBeFocused()
    await expect(box).toHaveValue("set honey to")
    // Said once, politely: a screen reader hears that a review is ready and how to reach it.
    await expect(
      page.locator('[aria-live="polite"], [role="status"]').filter({ hasText: /^Review ready/ })
    ).toHaveCount(1)
    // From the composer, Approve is reachable by keyboard (presses measured both ways).
    const presses = async (key: "Tab" | "Shift+Tab") => {
      await box.focus()
      for (let i = 1; i <= 40; i++) {
        await page.keyboard.press(key)
        if (await approve.evaluate((el) => el === document.activeElement)) return i
      }
      return Infinity
    }
    const forward = await presses("Tab")
    const back = await presses("Shift+Tab")
    console.log(`[P9-4a] ${page.viewportSize()!.width}px: Approve after Tab ×${forward}, Shift+Tab ×${back}`)
    expect(Math.min(forward, back), "Approve unreachable by keyboard from the composer").toBeLessThan(Infinity)
  })

  test("P9-4c: Switch to voice from the dock inside a scene keeps focus in the dock", async ({
    page,
  }) => {
    await fakeAgent(page)
    await page.goto("/")
    await startText(page)
    await page.getByRole("button", { name: s.backHome }).first().click()
    await page.getByRole("region", { name: sc.yourFlow }).getByRole("link").first().click()
    await expect(page).toHaveURL(/\/scenes\//)
    await pressByKeyboard(page.getByRole("button", { name: s.switchVoice }))
    await page.waitForTimeout(800)
    expect(await onBody(page), "focus lost to <body> after Switch to voice in a scene").toBe(false)
    await expect(page.getByLabel(v.message)).toBeFocused()
  })

  test("P9-4b: Switch to voice by keyboard keeps focus on the page", async ({ page }) => {
    await fakeAgent(page)
    await page.goto("/")
    await startText(page)
    const upgrade = page.getByRole("button", { name: /Switch to voice|Talk instead/ }).first()
    await pressByKeyboard(upgrade)
    await page.waitForTimeout(800)
    expect(await onBody(page), "focus lost to <body> after Switch to voice").toBe(false)
  })
})

test("P9-5: the explorer's tab panel shows keyboard focus with the same ring as a button", async ({
  page,
}) => {
  await seedSession(page)
  await mockApi(page, discoveryApi(OWNER))
  await page.goto("/explore")
  const panel = page.getByRole("tabpanel").first()
  await expect(panel).toBeVisible()
  const look = await focusLook(panel)
  expect(look.visible).toBe(true)
  expect(look.ring, "no visible focus on the tab panel").not.toBe("none")
})

test("P9-6: Shift+Tab from the dock reaches the orb only where it can be seen (home scrolled down)", async ({
  page,
  api,
}) => {
  void api
  await page.goto("/")
  await expect(page.getByRole("region", { name: sc.yourFlow })).toBeVisible()
  await page.locator("[data-flow-scroll]").evaluate((el) => el.scrollTo(0, el.scrollHeight))
  const orb = page.getByRole("button", { name: s.orbStart })
  await page.getByRole("button", { name: v.send, exact: true }).focus()
  let reached = false
  for (let i = 0; i < 30 && !reached; i++) {
    await page.keyboard.press("Shift+Tab")
    reached = await orb.evaluate((el) => el === document.activeElement).catch(() => false)
  }
  expect(reached, "orb unreachable by Shift+Tab").toBe(true)
  // Drawn where it is: inside the viewport and not hidden under anything (WCAG 2.4.11).
  const onScreen = await page.evaluate(() => {
    const el = document.activeElement as HTMLElement
    const r = el.getBoundingClientRect()
    if (r.bottom <= 0 || r.top >= innerHeight || r.right <= 0 || r.left >= innerWidth) return false
    const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2)
    return !!hit && (hit === el || el.contains(hit))
  })
  expect(onScreen, "the focused orb is off screen").toBe(true)
})
