// P7-3: the agent calls a tool on its own (no user turn) and then stays silent (SIMULATED voice).
import { expect, test } from "./storefront-api"
import { fakeAgent, startText, v } from "./voice"

test.beforeEach(({ api }) => void api)

test("Thinking… clears quietly when the agent goes silent after a tool call of its own", async ({
  page,
}) => {
  await page.clock.install()
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  const busy = page.locator('[aria-busy="true"]')

  // E.g. the status wait the agent starts after a review card, or a read after a check-in.
  agent.call("orders_list", {})
  await expect.poll(() => agent.results()).toBe(1)
  await expect(busy.first()).toBeVisible()
  await page.clock.runFor(30_000)
  await expect(busy.first()).toBeVisible()

  await page.clock.runFor(30_000)
  await expect(busy).toHaveCount(0)
  // Nobody asked, so there is no "couldn't finish that answer" to show.
  await expect(page.getByText(v.unfinished.stalled)).toHaveCount(0)
})
