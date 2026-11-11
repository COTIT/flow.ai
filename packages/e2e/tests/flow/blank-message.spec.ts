import { expect, test, designs } from "./storefront-api"
import { fakeAgent, send, startText } from "./voice"

// SIMULATED voice. P4-1: zero-width marks look empty, and `trim()` keeps them.

test("P4-1: a zero-width-only message is not sent", async ({ page, api }) => {
  designs(api)
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  await send(page, "\u200b\u200c\u200d\u2060\ufeff")
  await send(page, "Sales today?")
  // The real message proves the dock sent; the blank one before it never left.
  await expect
    .poll(() => agent.sent.filter((e) => e.type === "user_message").length)
    .toBe(1)
  expect(agent.sent.find((e) => e.type === "user_message")?.text).toBe(
    "Sales today?"
  )
})
