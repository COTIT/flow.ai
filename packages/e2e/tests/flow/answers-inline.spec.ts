// P5: an answer shows inline under Genesy's reply as one small card; the Results screen is only
// "Show all" for an answer too big for it. SIMULATED voice + API.
import { s } from "./shell"
import { expect, test } from "./storefront-api"
import { chat, fakeAgent, typeInstead, v } from "./voice"

test.beforeEach(({ api }) => void api)

async function talking(page: import("@playwright/test").Page) {
  const agent = await fakeAgent(page, (text, a) =>
    setTimeout(() => a.say(`Noted: ${text}`), 120)
  )
  await page.goto("/")
  await page.getByRole("button", { name: typeInstead, exact: true }).click()
  await page.getByLabel(v.message).fill("Which orders wait?")
  await page.getByRole("button", { name: v.send, exact: true }).click()
  await expect(
    page.getByText("Noted: Which orders wait?").first()
  ).toBeVisible()
  return agent
}

test("a small answer shows its card under the reply, with no hop to another screen", async ({
  page,
}) => {
  const agent = await talking(page)
  agent.call("orders_list")
  await expect.poll(agent.results).toBe(1)
  const log = page.getByRole("log").first()
  await expect(log.getByText("ORD-0042").first()).toBeVisible()
  await expect(log.getByRole("button", { name: chat.showResult })).toHaveCount(
    0
  )
  await expect(log.getByRole("button", { name: "Show all" })).toHaveCount(0)
  await expect(page).toHaveURL(/\/$/)
})

test("an answer too big for the card offers Show all, which opens the Results screen on it", async ({
  page,
}) => {
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
        view: {
          kind: "records",
          resource: "orders",
          total: 5,
          href: "/orders",
          rows,
        },
      },
    })
  })
  const agent = await talking(page)
  agent.call("orders_list")
  await expect.poll(agent.results).toBe(1)
  const all = page
    .getByRole("log")
    .first()
    .getByRole("button", { name: "Show all" })
  await expect(all).toBeVisible()
  await all.click()
  await expect(page).toHaveURL(/\/\?view=task$/)
  await expect(page.getByRole("region", { name: chat.results })).toBeVisible()
  await page.getByRole("button", { name: s.backHome }).first().click()
  await expect(page).toHaveURL(/\/$/)
})

test("while Genesy thinks the line says so without a running seconds counter", async ({
  page,
}) => {
  await fakeAgent(page, (text, a) => setTimeout(() => a.say(`Noted: ${text}`), 4_000))
  await page.goto("/")
  await page.getByRole("button", { name: typeInstead, exact: true }).click()
  await page.getByLabel(v.message).fill("Which orders wait?")
  await page.getByRole("button", { name: v.send, exact: true }).click()
  const thinking = page.locator("[data-voice-thinking]").first()
  await expect(thinking).toBeVisible()
  await page.waitForTimeout(1_500)
  await expect(thinking).not.toContainText(/\d\s?(s|seconds)\b/)
})
