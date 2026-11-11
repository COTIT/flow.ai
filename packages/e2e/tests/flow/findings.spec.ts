// Open findings from the Flow stress sweep: each test reproduces one break and is marked
// test.fail() with its id until a fix lands (then the fixer drops the mark). SIMULATED API + voice.
// Findings list: side-session notes, child-stress-test-findings.md.
import { expect, test } from "./storefront-api"
import { readBack, receipt, scene, stockReview, stockRoutes } from "./task"
import { fakeAgent, startText, v } from "./voice"

test.beforeEach(({ api }) => void api)


const stockReadBack = (
  api: Parameters<Parameters<typeof test>[2]>[0]["api"]
) => {
  const stock =
    readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
  api.on(
    "GET",
    "/workspace/:ws/products/:id/stock-with-bins",
    () => stock.body as object
  )
}
const propose = (agent: Awaited<ReturnType<typeof fakeAgent>>) =>
  agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 120 }] },
  })

test("SW4: after Approve from the keyboard, focus stays in the task (never lost to the page)", async ({
  page,
  api,
}) => {
  stockReadBack(api)
  const agent = await fakeAgent(page)
  await stockRoutes(page)
  await page.goto("/")
  await startText(page)
  propose(agent)
  const approve = scene(page).getByRole("button", { name: v.approve })
  await approve.focus()
  await page.keyboard.press("Enter")
  await expect(receipt(page)).toBeVisible()
  // Focus moves to what replaced the button (the receipt), not to <body>.
  await expect
    .poll(() => page.evaluate(() => document.activeElement !== document.body))
    .toBe(true)
})

// The review never takes focus from someone typing. (The dock's field itself still loses focus when
// the task scene opens: a separate layout finding, reported to the parent.)
test("SW4: a review proposed while the person types never takes their focus", async ({
  page,
  api,
}) => {
  stockReadBack(api)
  const agent = await fakeAgent(page)
  await stockRoutes(page)
  await page.goto("/")
  await startText(page)
  const message = page.getByLabel(v.message)
  await message.fill("and also")
  propose(agent)
  await expect(scene(page)).toContainText("Confirm action 135790")
  await page.waitForTimeout(300)
  await expect(page.locator("#flow-review-title")).not.toBeFocused()
})

test("SW4: a review that opens takes focus from the page", async ({ page, api }) => {
  stockReadBack(api)
  const agent = await fakeAgent(page)
  await stockRoutes(page)
  await page.goto("/")
  await startText(page)
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur())
  propose(agent)
  await expect(page.locator("#flow-review-title")).toBeFocused()
})

test("SW5: a review that expires while open says so where the card was", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  await page.route("**/api/voice/tools", (route) => {
    const body = route.request().postDataJSON() as {
      command?: string
      operationId?: string
    }
    if (body.command !== "execute" || body.operationId !== "stock_set_levels")
      return route.fallback()
    const review = stockReview(120)
    review.review.expiresAt = new Date(Date.now() + 2_000).toISOString()
    return route.fulfill({ json: review })
  })
  await page.goto("/")
  await startText(page)
  propose(agent)
  await expect(scene(page)).toContainText("Confirm action 135790")
  // The dock already says it; the scene the person is looking at must not go blank.
  await expect(scene(page)).toContainText("This review expired", {
    timeout: 6_000,
  })
  // One way forward: asking again sends one plain request for a fresh proposal.
  await scene(page).getByRole("button", { name: "Ask again" }).dblclick()
  await expect
    .poll(() => agent.sent.filter((e) => e.type === "user_message").length)
    .toBe(1)
  await expect(scene(page)).not.toContainText("This review expired")
})
