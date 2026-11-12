// P4-1: a draft of only zero-width marks (they survive trim()) is not a message: it starts nothing,
// sends nothing and draws no bubble. Real text with an emoji joiner still goes. SIMULATED voice.
import { expect, test } from "./storefront-api"
import { fakeAgent, startText, v, type Agent } from "./voice"

test.beforeEach(({ api }) => void api)

const BLANK = "​‌‍⁠﻿"
const sent = (agent: Agent) =>
  agent.sent.filter((e) => e.type === "user_message").map((e) => String(e.text))
const userRows = (page: import("@playwright/test").Page) =>
  page.getByRole("log").locator('[data-role="user"]')

test("P4-1: a zero-width-only draft on home starts no conversation", async ({ page }) => {
  const agent = await fakeAgent(page)
  await page.goto("/")
  const box = page.getByLabel(v.message)
  await box.fill(BLANK)
  await box.press("Enter")
  await page.waitForTimeout(800)
  expect(agent.connections, "no voice session for nothing").toBe(0)
  expect(sent(agent)).toEqual([])
})

test("P4-1: in a conversation, zero-width-only sends nothing; text with an emoji ZWJ sends", async ({
  page,
}) => {
  const agent = await fakeAgent(page, (text, a) => a.say(`Noted: ${text}`))
  await page.goto("/")
  await startText(page)
  const box = page.getByLabel(v.message)
  await box.fill(BLANK)
  await box.press("Enter")
  await page.waitForTimeout(500)
  expect(sent(agent)).toEqual([])
  await expect(userRows(page)).toHaveCount(0)

  const real = "Thanks 👩‍💻"
  await box.fill(real)
  await box.press("Enter")
  await expect(page.getByRole("log")).toContainText(`Noted: ${real}`)
  expect(sent(agent)).toEqual([real])
  await expect(userRows(page)).toHaveCount(1)
})
