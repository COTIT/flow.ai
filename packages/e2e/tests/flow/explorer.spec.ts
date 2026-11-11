import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { mockApi } from "./api"
import {
  CLERK,
  OWNER,
  api,
  d,
  label,
  pills,
  stubScenes,
  visitExplorer,
} from "./discovery"
import { fakeAgent, watch } from "./voice"

// SAMPLE data and a SIMULATED agent: mock API, mock voice upstream, fake ElevenLabs socket.

test.beforeEach(async ({ page }) => {
  await seedSession(page)
})

const card = (
  page: import("@playwright/test").Page,
  id: keyof typeof d.capabilities
) => page.locator(`[data-capability="${id}"]`)

test("four groups, switched by click and by arrow keys", async ({ page }) => {
  await mockApi(page, api(OWNER))
  await page.goto("/explore")
  const tab = (group: keyof typeof d.explorer.groups) =>
    page.getByRole("tab", { name: d.explorer.groups[group] })
  await expect(tab("understand")).toHaveAttribute("aria-selected", "true")
  await expect(card(page, "morning_briefing")).toBeVisible()
  await tab("automate").click()
  await expect(card(page, "schedule_weekly_report")).toBeVisible()
  await expect(card(page, "morning_briefing")).toBeHidden()
  await page.keyboard.press("ArrowRight")
  await expect(tab("create")).toBeFocused()
  await page.keyboard.press("Enter")
  await expect(card(page, "storefront_design")).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test("statuses in words: available, requires approval, planned", async ({
  page,
}) => {
  await mockApi(page, api(OWNER))
  await page.goto("/explore")
  await expect(card(page, "weekly_sales")).toContainText(d.status.available)
  await page.getByRole("tab", { name: d.explorer.groups.operate }).click()
  const adjust = card(page, "adjust_stock")
  await expect(adjust).toContainText(d.status.approval)
  await expect(adjust).toContainText(d.statusText.approval)
  await page.getByRole("tab", { name: d.explorer.groups.automate }).click()
  const planned = card(page, "conditional_monitoring")
  await expect(planned).toContainText(d.status.planned)
  await expect(planned.getByRole("button")).toHaveCount(0)
})

test("a role sees what it cannot use as restricted, with nothing to try", async ({
  page,
}) => {
  await mockApi(page, api(CLERK))
  await page.goto("/explore")
  await page.getByRole("tab", { name: d.explorer.groups.operate }).click()
  const stock = card(page, "review_low_stock")
  await expect(stock).toContainText(d.status.restricted)
  await expect(stock).toContainText(d.statusText.restricted)
  await expect(stock.getByRole("button")).toHaveCount(0)
  await expect(card(page, "find_product")).toContainText(d.status.available)
})

test("Try it on a screen opens it directly; on an instruction it asks Genesy in text", async ({
  page,
}) => {
  await mockApi(page, api(OWNER))
  await stubScenes(page)
  const sessions = watch(page, "session")
  const agent = await fakeAgent(page)
  await page.goto("/explore")
  await card(page, "weekly_sales")
    .getByRole("button", { name: `Try ${label("weekly_sales")}` })
    .click()
  await expect
    .poll(() => JSON.stringify(agent.sent))
    .toContain(d.capabilities.weekly_sales.prompt)
  expect(sessions.filter((r) => r.method === "POST")).toHaveLength(1)
  await card(page, "morning_briefing")
    .getByRole("button", { name: `Try ${label("morning_briefing")}` })
    .click()
  await expect(page).toHaveURL(/\/scenes\/pulse$/)
  expect(sessions.filter((r) => r.method === "POST")).toHaveLength(1)
})

test("Reset suggestions forgets what was learned on this device", async ({
  page,
}) => {
  await mockApi(page, api(OWNER))
  await visitExplorer(page, 3)
  await page.goto("/")
  await expect(pills(page).first()).toHaveText(label("explore"))
  await page.goto("/explore")
  await page.getByRole("button", { name: d.explorer.reset }).click()
  await expect(
    page.getByRole("status").filter({ hasText: d.explorer.resetDone })
  ).toBeVisible()
  await page.goto("/")
  await expect(pills(page).first()).toHaveText(label("morning_briefing"))
})
