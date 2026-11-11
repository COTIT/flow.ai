// P9-3: switching workspace from the header by keyboard keeps focus on the picker and says so.
// SIMULATED API (mockApi).
import { seedSession } from "../../src/session"
import { expect, test } from "../../src/test"
import { flowApi, ILUNDO, mockApi, RUAHA } from "./api"
import { revealHeader } from "./shell"

test.beforeEach(async ({ page }) => {
  await seedSession(page, { workspaceIds: ["w1", "w2"] })
  await mockApi(page, flowApi({ "GET /me/workspaces": { body: { data: [ILUNDO, RUAHA] } } }))
})

test("P9-3 choosing another workspace by keyboard keeps focus on the picker and announces it once", async ({
  page,
}) => {
  await page.goto("/")
  const menu = (await revealHeader(page)).getByRole("combobox", { name: "Workspace" })
  await menu.focus()

  // Open, move, Escape: nothing changes and focus is back on the picker.
  await page.keyboard.press("Enter")
  await expect(page.getByRole("listbox")).toBeVisible()
  // Opened: the current workspace takes focus first, so the arrow moves from it.
  await expect(page.getByRole("option", { name: ILUNDO.display_name })).toBeFocused()
  await page.keyboard.press("ArrowDown")
  await page.keyboard.press("Escape")
  await expect(page.getByRole("listbox")).toHaveCount(0)
  await expect(menu).toBeFocused()

  // Open, move, choose: the switch happens, focus stays on the picker, and it is said once.
  await page.keyboard.press("Enter")
  await expect(page.getByRole("listbox")).toBeVisible()
  // Opened: the current workspace takes focus first, so the arrow moves from it.
  await expect(page.getByRole("option", { name: ILUNDO.display_name })).toBeFocused()
  await page.keyboard.press("ArrowDown")
  await expect(page.getByRole("option", { name: RUAHA.display_name })).toBeFocused()
  await page.keyboard.press("Enter")
  await expect(menu).toContainText(RUAHA.display_name)
  await expect(menu).toBeFocused()
  const said = page.getByRole("status").filter({ hasText: `Switched to ${RUAHA.display_name}` })
  await expect(said).toHaveCount(1)
  // The next Tab goes on from the picker, not from the top of the page.
  await page.keyboard.press("Tab")
  expect(await page.evaluate(() => document.activeElement === document.body)).toBe(false)
  await expect(page.locator("#flow-header").locator(":focus")).toHaveCount(1)
})
