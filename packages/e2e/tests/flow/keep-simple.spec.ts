// Keep Flow simple (user rule, 2026-11-12) over a SIMULATED API: the scene bar has no Menu
// button (the header opens by edge, Tab, swipe or the handle), the handle is the visible, named
// way to the menu on every device, the swipe hint only shows on the start screen, and list
// tables carry no visible "Actions" header.
import type { Page } from "@playwright/test"

import { sc, sceneWorld } from "./scenes-api"
import { s } from "./shell"
import { expect, test } from "./storefront-api"

test.beforeEach(({ api }) => sceneWorld(api))

const header = (page: Page) => page.locator("[data-flow-header]")
const handle = (page: Page) => page.locator("[data-flow-handle]")

/** WCAG contrast of the handle's bar against the page behind it (non-text: at least 3:1). */
async function handleContrast(page: Page) {
  return page.evaluate(() => {
    const rgba = (color: string) => {
      const c = document.createElement("canvas").getContext("2d")!
      c.fillStyle = color
      c.fillRect(0, 0, 1, 1)
      return [...c.getImageData(0, 0, 1, 1).data].map((v, i) => (i < 3 ? v : v / 255))
    }
    const bar = document.querySelector("[data-flow-handle] span")!
    let back = getComputedStyle(document.body).backgroundColor
    for (let n: Element | null = bar.parentElement; n; n = n.parentElement) {
      const bg = getComputedStyle(n).backgroundColor
      if (rgba(bg)[3]! > 0) {
        back = bg
        break
      }
    }
    const [br, bgc, bb] = rgba(back)
    const [r, g, b, a] = rgba(getComputedStyle(bar).backgroundColor)
    const over = [r! * a! + br! * (1 - a!), g! * a! + bgc! * (1 - a!), b! * a! + bb! * (1 - a!)]
    const lum = (rgb: number[]) => {
      const [x, y, z] = rgb.map((v) => {
        const s = v / 255
        return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4
      })
      return 0.2126 * x! + 0.7152 * y! + 0.0722 * z!
    }
    const [l1, l2] = [lum(over), lum([br!, bgc!, bb!])].sort((p, q) => q - p)
    return (l1! + 0.05) / (l2! + 0.05)
  })
}

test("the scene bar has no Menu button; no task line while no task exists", async ({ page }) => {
  await page.goto("/scenes/stock")
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
  await expect(page.getByRole("button", { name: s.menu, exact: true })).toHaveCount(0)
  await expect(page.locator("[data-task-state]")).toHaveCount(0)
})

for (const scheme of ["light", "dark"] as const)
  test(`the handle is the visible, named way to the menu (${scheme})`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme })
    await page.goto("/scenes/stock")
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
    const show = page.getByRole("button", { name: s.showHeader, exact: true })
    await expect(show).toBeVisible()
    await expect(show).toHaveAttribute("title", s.showHeader)
    const box = (await show.boundingBox())!
    expect(box.height).toBeGreaterThanOrEqual(44)
    expect(box.width).toBeGreaterThanOrEqual(44)
    expect(await handleContrast(page)).toBeGreaterThanOrEqual(3)
    await show.click()
    await expect(header(page)).toHaveAttribute("data-shown", "true")
    await expect(handle(page)).toHaveAccessibleName(s.hideHeader)
    await expect(handle(page)).toHaveAttribute("title", s.hideHeader)
  })

test("the swipe hint never covers a scene: it waits for the start screen", async ({ page }) => {
  test.skip(!test.info().project.use.hasTouch, "the hint is for touch")
  await page.goto("/scenes/stock")
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
  await page.waitForTimeout(500)
  await expect(page.getByText(s.headerHint)).toHaveCount(0)
  // Not used up on a scene: it still shows the first time home opens.
  expect(await page.evaluate(() => localStorage.getItem("flow.headerHint.seen"))).toBeNull()
  await page.getByRole("button", { name: s.backHome }).first().click()
  await expect(page).toHaveURL(/\/$/)
  await expect(page.getByText(s.headerHint)).toBeVisible()
})

test("Needs your attention: the buttons speak for themselves, no visible Actions header", async ({
  page,
}) => {
  await page.goto("/scenes/noticed")
  await expect(page.getByRole("button", { name: /^Restock / }).first()).toBeVisible()
  const actions = page.getByRole("columnheader", { name: sc.table.actions })
  // Still named for screen readers, drawn for nobody (sr-only is a 1 px clip).
  if (await actions.count()) {
    const label = actions.first().getByText(sc.table.actions)
    expect((await label.boundingBox())?.width ?? 0).toBeLessThanOrEqual(1)
  }
})
