// The Results screen as an address (SIMULATED voice): reaching it from a scene, and leaving it.
import type { Page } from "@playwright/test"

import { s } from "./shell"
import { expect, test } from "./storefront-api"
import { chat, fakeAgent, v } from "./voice"

test.beforeEach(({ api }) => void api)

const results = (page: Page) => page.getByRole("region", { name: chat.results })

/** An orders answer too big for its inline card (5 rows): it offers "Show all" (P5). */
async function bigAnswer(page: Page) {
  const rows = Array.from({ length: 5 }, (_, i) => ({
    id: `o-${i}`,
    label: `ORD-01${i}`,
    detail: "Neema Shop · Open",
    amount: { cents: 100_000 + i, currency: "USD", decimals: 2 },
  }))
  await page.route("**/api/voice/tools", async (route) => {
    const body = route.request().postDataJSON() as { operationId?: string }
    if (body?.operationId !== "orders_list") return route.fallback()
    await route.fulfill({
      json: {
        status: "completed",
        data: { total: 5 },
        view: { kind: "records", resource: "orders", total: 5, href: "/orders", rows },
      },
    })
  })
}
const showAll = (page: Page) =>
  page.getByRole("log").first().getByRole("button", { name: "Show all" }).click()

/** A text conversation started with "Type instead", one turn answered. */
async function talking(page: Page) {
  await bigAnswer(page)
  const agent = await fakeAgent(page, (text, a) =>
    setTimeout(() => a.say(`Found: ${text}`), 120)
  )
  await page.goto("/")
  await page.getByRole("button", { name: s.typeInstead }).click()
  await page.locator("[data-flow-composer]").fill("Hello")
  await page.getByRole("button", { name: v.send, exact: true }).click()
  await expect(page.getByText("Found: Hello").first()).toBeVisible()
  return agent
}

test("J1: Show in results from a scene's conversation opens the Results screen", async ({
  page,
}) => {
  const agent = await talking(page)
  await page.getByRole("button", { name: s.backHome }).first().click()
  await page
    .getByRole("region", { name: /Your Flow/ })
    .getByRole("link", { name: /Sales Perspective/ })
    .click()
  await expect(page).toHaveURL(/\/scenes\/sales$/)
  agent.call("orders_list")
  await expect.poll(agent.results).toBe(1)
  await showAll(page) // P5: answers show inline; a big one opens the Results screen
  await expect(page).toHaveURL(/\/\?view=task$/)
  await expect(results(page)).toBeVisible()
})

test("J4: Back to home from the Results screen lands on the home screen, not the conversation", async ({
  page,
}) => {
  const agent = await talking(page)
  agent.call("orders_list")
  await expect.poll(agent.results).toBe(1)
  await showAll(page) // P5: answers show inline; a big one opens the Results screen
  await expect(results(page)).toBeVisible()
  await page.getByRole("button", { name: s.backHome }).first().click()
  await expect(results(page)).toBeHidden()
  await expect(page).toHaveURL(/\/$/)
  await expect(
    page.getByText(s.continueConversation, { exact: true })
  ).toBeVisible()
  await expect(
    page.locator("[data-flow-composer]"),
    "home, not the conversation"
  ).toBeHidden()
})
