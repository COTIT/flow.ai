import type { Page } from "@playwright/test"

import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { chat, fakeAgent, FLOW_ORDER, OFFICE, startText, v } from "./voice"
import { s } from "./shell"

// SIMULATED voice: mock upstream behind Flow's real proxy + a fake ElevenLabs socket.

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi())
})

/** Opens the result scene from the nth result line of the conversation. */
async function results(page: Page, line: number) {
  await page
    .getByRole("log")
    .getByRole("button", { name: chat.showResult })
    .nth(line)
    .click()
  return page.getByRole("region", { name: chat.results })
}

test("before any answer there is no result scene and no placeholder chart", async ({
  page,
}) => {
  await page.goto("/")
  await expect(page.getByText(v.welcome)).toBeVisible()
  await expect(page.getByRole("region", { name: chat.results })).toHaveCount(0)
  await expect(page.locator("[data-chart]")).toHaveCount(0)
  await expectNoHorizontalScroll(page)
})

test("a list answer shows in the results with record links to Office in a new tab", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  agent.call("orders_list")
  await expect.poll(agent.results).toBe(1)
  const line = page.getByRole("log").getByRole("button", {
    name: chat.showResult,
  })
  await expect(line).toBeVisible()
  const pane = await results(page, 0)
  const row = pane.getByRole("link", { name: /ORD-0042/ })
  await expect(row).toBeVisible()
  await expect(row).toHaveAttribute("href", `${OFFICE}/orders/${FLOW_ORDER}`)
  await expect(row).toHaveAttribute("target", "_blank")
  await expect(row).toHaveAttribute("rel", "noopener noreferrer")
  await expect(
    pane.getByRole("link", { name: v.results.openAll })
  ).toHaveAttribute("href", `${OFFICE}/orders`)
  await expectNoHorizontalScroll(page)
  // Back from the scene returns to the same conversation.
  await page.getByRole("button", { name: s.backHome }).click()
  await expect(pane).toBeHidden()
  await expect(line).toBeVisible()
})

test("a sales answer draws its chart in the results", async ({ page }) => {
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  agent.call("orders_sales_summary")
  await expect.poll(agent.results).toBe(1)
  const pane = await results(page, 0)
  await expect(pane.locator("[data-chart]")).toContainText("TZS 1,250,000")
  await expect(pane.locator("[data-chart]")).toContainText("42 orders")
  await expectNoHorizontalScroll(page)
})

test("links go only where Flow allows: Office records, Flow's storefront; the rest stays text", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  const log = page.getByRole("log")

  agent.call("office_page_open", { page: "storefront" })
  const storefront = log.getByRole("link", { name: /Storefront design/ })
  await expect(storefront).toHaveAttribute("href", "/storefront")

  agent.call("office_page_open", { page: "orders" })
  await expect(log.getByRole("link", { name: /Orders/ })).toHaveAttribute(
    "href",
    `${OFFICE}/orders`
  )

  // Agent-text links pass Flow's policy: an Office record maps to Office, anything else stays text.
  agent.say(
    `See [Order 42](/orders/${FLOW_ORDER}) or [Steal this](https://evil.example/steal) or [Password](/settings/password).`
  )
  const line = log.locator('[data-role="agent"]').last()
  await expect(line.getByRole("link", { name: "Order 42" })).toHaveAttribute(
    "href",
    `${OFFICE}/orders/${FLOW_ORDER}`
  )
  await expect(line).toContainText("Steal this")
  await expect(line).toContainText("Password")
  await expect(line.getByRole("link")).toHaveCount(1)
  await expect(page.locator('a[href*="/settings/password"]')).toHaveCount(0)
  await expect(page.locator('a[href*="evil.example"]')).toHaveCount(0)
})

test("a secure-form task becomes an Open in Office action with no token in it", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  agent.call("app_keys_create")
  const notice = page.getByRole("status").filter({ hasText: chat.secureTitle })
  await expect(notice).toBeVisible()
  const open = notice.getByRole("link", { name: chat.openInOffice })
  await expect(open).toHaveAttribute(
    "href",
    `${OFFICE}/voice/secure?operation=app_keys_create`
  )
  await expect(open).toHaveAttribute("target", "_blank")
  await expect(open).toHaveAttribute("rel", "noopener noreferrer")
  await expect(notice).toContainText(chat.officeSignIn)
  await expectNoHorizontalScroll(page)
})
