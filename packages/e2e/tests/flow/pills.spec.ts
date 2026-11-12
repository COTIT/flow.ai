import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { mockApi } from "./api"
import {
  CLERK,
  LOW_STOCK,
  OWNER,
  api,
  d,
  label,
  pills,
  stubScenes,
  visitExplorer,
} from "./discovery"
import { dock, expectOrbClear } from "./shell"
import { chat, fakeAgent, startText, v, watch } from "./voice"

// SAMPLE data and a SIMULATED agent: mock API, mock voice upstream, fake ElevenLabs socket.

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await page.addInitScript(() => {
    Reflect.set(window, "micRequests", 0)
    const real = navigator.mediaDevices?.getUserMedia?.bind(
      navigator.mediaDevices
    )
    if (real)
      navigator.mediaDevices.getUserMedia = (constraints) => {
        Reflect.set(
          window,
          "micRequests",
          Number(Reflect.get(window, "micRequests")) + 1
        )
        return real(constraints)
      }
  })
})

test("home suggests by slot: contextual, Flow noticed, unfamiliar; no session on load", async ({
  page,
}) => {
  await mockApi(page, api(OWNER, LOW_STOCK))
  const sessions = watch(page, "session")
  const agent = await fakeAgent(page)
  await page.goto("/")
  await expect(pills(page)).toHaveText([
    label("morning_briefing"),
    label("review_low_stock"),
    label("review_alerts"),
    label("pending_orders"),
  ])
  await expect(dock(page)).toContainText(d.suggested)
  expect(sessions).toHaveLength(0)
  expect(agent.connections).toBe(0)
})

test("a screen pill opens its scene directly: no session, no AI", async ({
  page,
}) => {
  await mockApi(page, api(OWNER, LOW_STOCK))
  await stubScenes(page)
  const sessions = watch(page, "session")
  const agent = await fakeAgent(page)
  await page.goto("/")
  await pills(page)
    .filter({ hasText: label("review_low_stock") })
    .click()
  await expect(page).toHaveURL(/\/scenes\/stock$/)
  expect(sessions).toHaveLength(0)
  expect(agent.connections).toBe(0)
})

test("an instruction pill starts a text conversation and sends it, never the microphone", async ({
  page,
}) => {
  await mockApi(page, api(OWNER))
  const sessions = watch(page, "session")
  const agent = await fakeAgent(page, (text, a) => a.say(`You said: ${text}`))
  await page.goto("/explore")
  await pills(page)
    .filter({ hasText: label("weekly_sales") })
    .click()
  await expect
    .poll(() => JSON.stringify(agent.sent))
    .toContain(d.capabilities.weekly_sales.prompt)
  expect(sessions.filter((r) => r.method === "POST")).toHaveLength(1)
  expect(agent.connections).toBe(1)
  expect(await page.evaluate(() => Reflect.get(window, "micRequests"))).toBe(0)
  await expect(dock(page).getByLabel(v.message)).toHaveValue("")
})

test("a role never sees suggestions it cannot use, even with matching alerts", async ({
  page,
}) => {
  await mockApi(page, api(CLERK, LOW_STOCK))
  await page.goto("/")
  await expect(pills(page)).toHaveText([
    label("morning_briefing"),
    label("review_alerts"),
    label("explore"),
    label("find_product"),
  ])
  for (const hidden of [
    "review_low_stock",
    "pending_orders",
    "sales_perspective",
    "storefront_design",
  ] as const)
    await expect(
      dock(page).getByRole("button", { name: label(hidden) })
    ).toHaveCount(0)
})

test("suggestions learn from use: three explorer visits make it the first pill", async ({
  page,
}) => {
  await mockApi(page, api(OWNER))
  await page.goto("/")
  await expect(pills(page).first()).toHaveText(label("morning_briefing"))
  await visitExplorer(page, 3)
  await page.goto("/")
  await expect(pills(page)).toHaveText([
    label("explore"),
    label("morning_briefing"),
    label("review_alerts"),
    label("pending_orders"),
  ])
})

test("after a completed action: here's what you can do next", async ({
  page,
}) => {
  await mockApi(page, api(OWNER))
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  agent.call("orders_create", { body: { reference: "Flow e2e order" } })
  const review = page.getByRole("region", { name: chat.results })
  await expect(review).toContainText("Flow e2e order")
  await review.getByRole("button", { name: v.approve }).click()
  // F4: the decided review becomes its receipt in place.
  await expect(review.getByRole("button", { name: v.approve })).toHaveCount(0)
  await expect(dock(page)).toContainText(d.next)
  await expect(pills(page)).toHaveText([label("pending_orders")])
})

test("first-visit hint: under the orb, about this role, dismissed for good", async ({
  page,
}) => {
  await mockApi(page, api(OWNER))
  await page.goto("/")
  const hint = page.getByRole("complementary", { name: d.hint.lead })
  await expect(hint).toContainText(d.hint.areas.stock)
  await expect(hint).toContainText(d.hint.areas.orders)
  await expectOrbClear(page, [hint, dock(page)])
  await hint.getByRole("button", { name: d.hint.dismiss }).click()
  await expect(hint).toHaveCount(0)
  await page.reload()
  await expect(pills(page).first()).toBeVisible()
  await expect(hint).toHaveCount(0)
})

test("the hint opens the explorer; the composer does not move and nothing scrolls sideways", async ({
  page,
}) => {
  await mockApi(page, api(OWNER))
  await page.goto("/")
  // Orb-first home's dock holds only the suggestions; its bottom edge must not jump.
  const box = dock(page)
  await expect(pills(page)).toHaveCount(4)
  const before = await box.boundingBox()
  await page.getByRole("link", { name: d.hint.explore }).click()
  await expect(
    page.getByRole("heading", { name: d.explorer.title })
  ).toBeVisible()
  await expect(pills(page)).toHaveCount(4)
  const after = await box.boundingBox()
  expect(Math.round(after!.y + after!.height)).toBe(Math.round(before!.y + before!.height))
  await expectNoHorizontalScroll(page)
  if ((page.viewportSize()?.width ?? 0) < 600) {
    const row = dock(page).getByRole("list", { name: d.suggested })
    expect(await row.evaluate((el) => el.scrollWidth > el.clientWidth)).toBe(
      true
    )
  }
})
