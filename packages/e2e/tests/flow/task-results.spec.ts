import { scene } from "./task"
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

/** The nth answer of the conversation: inline under its reply (P5), no separate screen. */
async function results(page: Page, line: number) {
  return page.getByRole("log").first().locator("[data-inline-result]").nth(line)
}

test("before any answer there is no result scene and no placeholder chart", async ({
  page,
}) => {
  await page.goto("/")
  await expect(page.getByText(s.tapToTalk, { exact: true })).toBeVisible()
  await expect(scene(page)).toHaveCount(0)
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
  // P5: the answer is its own card under the reply, no button to another screen.
  const pane = await results(page, 0)
  await expect(pane).toBeVisible()
  const row = pane.getByRole("link", { name: /ORD-0042/ })
  await expect(row).toBeVisible()
  await expect(row).toHaveAttribute("href", `${OFFICE}/orders/${FLOW_ORDER}`)
  await expect(row).toHaveAttribute("target", "_blank")
  await expect(row).toHaveAttribute("rel", "noopener noreferrer")
  await expect(
    pane.getByRole("link", { name: v.results.openAll })
  ).toHaveAttribute("href", `${OFFICE}/orders`)
  await expectNoHorizontalScroll(page)
  // Back lands on home (J4); the orb's words continue the same conversation, its answer there.
  await page.getByRole("button", { name: s.backHome }).click()
  await expect(pane).toBeHidden()
  await page.getByText(s.continueConversation, { exact: true }).click()
  await expect(pane).toBeVisible()
})

test("a sales answer draws its chart in the results", async ({ page }) => {
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  agent.call("orders_sales_summary")
  await expect.poll(agent.results).toBe(1)
  const pane = await results(page, 0)
  await pane.getByRole("application").focus()
  await page.keyboard.press("ArrowRight")
  await expect(pane.getByRole("tooltip")).toContainText(/TZS\s1,250,000/)
  await expect(pane.getByRole("tooltip")).toContainText("42 orders")
  await expectNoHorizontalScroll(page)
})

test("daily sales over a week draw as a trend line, never a progress bar per day", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  const days = ["2026-11-03", "2026-11-04", "2026-11-05", "2026-11-06", "2026-11-07"]
  await page.route("**/api/voice/tools", (route) => {
    const body = route.request().postDataJSON() as { command?: string; operationId?: string }
    if (body.command !== "execute" || body.operationId !== "orders_sales_daily")
      return route.fallback()
    return route.fulfill({
      json: {
        status: "completed",
        data: {},
        view: {
          kind: "sales_daily",
          from: days[0],
          to: days.at(-1),
          time_zone: "UTC",
          currencies: [
            {
              currency: "USD",
              decimals: 2,
              days: days.map((date, i) => ({ date, gross: 1000 * (i + 1), net: 1000 * (i + 1), orders: i + 1 })),
            },
          ],
        },
      },
    })
  })
  await page.goto("/")
  await startText(page)
  agent.call("orders_sales_daily", { query: { from: days[0], to: days.at(-1) } })
  const scene = await results(page, 0)
  const chart = scene.locator('[data-kind="sales_daily"]')
  await expect(chart.locator('[data-chart="line"] .recharts-line-curve')).toHaveCount(1)
  await expect(chart.locator(".recharts-bar-rectangle")).toHaveCount(0)
  // The numbers stay the API's, in the table behind the picture.
  await expect(chart.getByRole("row")).toHaveCount(days.length + 1)
  await expect(chart.getByRole("table")).toContainText("$50.00")
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
