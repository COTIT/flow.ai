// P2-3b: a scene's write buttons write once, however fast the second press comes (SIMULATED API).
import { menuItem, moreMenu, sc } from "./scenes-api"
import { expect, test } from "./storefront-api"

test.beforeEach(({ api }) => void api)

/** Two presses in the same task: no layout change or double-click detail can absorb them. */
const pressTwice = (name: string) =>
  [...document.querySelectorAll<HTMLElement>('button, [role="menuitem"]')]
    .filter((b) => b.textContent?.trim() === name)
    .forEach((b) => {
      b.click()
      b.click()
    })

test("Show pending orders first writes once and then says it is done", async ({ page, api }) => {
  await page.goto("/scenes/pulse")
  await moreMenu(page).click()
  const button = menuItem(page, sc.pendingFirst)
  await expect(button).toBeEnabled()
  await page.evaluate(pressTwice, sc.pendingFirst)
  await expect(page.getByRole("status").filter({ hasText: sc.pendingFirstDone })).toBeVisible()
  await page.waitForTimeout(500)
  expect(api.callsTo("PUT", "/workspace/w1/flow/scenes/pulse")).toHaveLength(1)
  // Already in that order: nothing left to do until the order changes.
  await moreMenu(page).click()
  await expect(button).toBeDisabled()
})

test("Remove from Your Flow writes once", async ({ page, api }) => {
  await page.goto("/scenes/pulse")
  await moreMenu(page).click()
  await expect(menuItem(page, sc.remove)).toBeEnabled()
  await page.evaluate(pressTwice, sc.remove)
  await expect(page).toHaveURL(/\/$/)
  expect(api.callsTo("PUT", "/workspace/w1/flow/scenes/pulse")).toHaveLength(1)
})
