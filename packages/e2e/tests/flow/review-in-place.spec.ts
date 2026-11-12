// P5: a review opens where the person is (the conversation beside the scene, the phone sheet, home's full
// conversation), with "Yes, do it" in thumb reach; its receipt replaces it in place. SIMULATED voice + API.
import type { Page } from "@playwright/test"

import { s } from "./shell"
import { expect, test } from "./storefront-api"
import { capsule, readBack, receipt, stockRoutes } from "./task"
import { fakeAgent, typeInstead, v, type Agent } from "./voice"

const card = (page: Page) => page.locator("[data-inline-task]:not(:empty)")
const yes = (page: Page) =>
  card(page).getByRole("button", { name: v.approve, exact: true })

async function stockWatchInConversation(
  page: Page,
  api: import("../office/fake-api").FakeApi
) {
  const stock =
    readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
  api.on(
    "GET",
    "/workspace/:ws/products/:id/stock-with-bins",
    () => stock.body as object
  )
  await stockRoutes(page)
  const agent = await fakeAgent(page, (text, a) =>
    setTimeout(() => a.say(`Noted: ${text}`), 120)
  )
  await page.goto("/")
  await page.getByRole("button", { name: typeInstead, exact: true }).click()
  await page.getByLabel(v.message).fill("Which products are low?")
  await page.getByRole("button", { name: v.send, exact: true }).click()
  await expect(
    page.getByText("Noted: Which products are low?").first()
  ).toBeVisible()
  await page.waitForTimeout(1600)
  await page.getByRole("button", { name: s.backHome }).first().click()
  await page
    .getByRole("region", { name: /Your Flow/ })
    .getByRole("link", { name: /Stock Watch/ })
    .click()
  await expect(page).toHaveURL(/\/scenes\/stock$/)
  return agent
}
const propose = (agent: Agent) =>
  agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 120 }] },
  })

/** Fully inside the viewport and the topmost thing at its centre (a thumb can press it). */
async function inReach(page: Page) {
  const box = (await yes(page).boundingBox())!
  const view = page.viewportSize()!
  expect(box.y, "top in view").toBeGreaterThanOrEqual(0)
  expect(box.y + box.height, "bottom in view").toBeLessThanOrEqual(view.height)
  expect(box.height, "a big target").toBeGreaterThanOrEqual(44)
  const hit = await yes(page).evaluate((el) => {
    const r = el.getBoundingClientRect()
    const top = document.elementFromPoint(
      r.left + r.width / 2,
      r.top + r.height / 2
    )
    return !!top && (top === el || el.contains(top))
  })
  expect(hit, "nothing covers it").toBe(true)
  const below = await page.evaluate(
    () => document.documentElement.scrollHeight - window.innerHeight
  )
  expect(below, "the page never scrolls").toBeLessThanOrEqual(0)
}

function reviewTests() {
  test("a review on a scene opens in place: Yes, do it in reach; the receipt replaces it; the URL stays", async ({
    page,
    api,
  }) => {
    const agent = await stockWatchInConversation(page, api)
    propose(agent)
    await expect(yes(page)).toBeVisible()
    await expect(page).toHaveURL(/\/scenes\/stock$/)
    await expect(card(page)).toContainText("120")
    await expect(
      card(page).getByRole("button", { name: v.cancel, exact: true })
    ).toBeVisible()
    await inReach(page)
    // The capsule would say the same thing: hidden while the card is on screen.
    await expect(capsule(page)).toBeHidden()
    await yes(page).click()
    await expect(receipt(page)).toBeVisible()
    await expect(page).toHaveURL(/\/scenes\/stock$/)
    // Only the sentence; the details fold.
    await expect(receipt(page)).toContainText("is now 120")
    await expect(
      receipt(page).getByText("Status", { exact: true })
    ).toBeHidden()
  })

  test("a review arriving takes keyboard focus to the card, never to the page", async ({
    page,
    api,
  }) => {
    const agent = await stockWatchInConversation(page, api)
    // A row action has focus when the review arrives (seed 1562502728's path).
    const restock = page.getByRole("button", { name: /^Restock / }).first()
    if (await restock.isVisible()) await restock.focus()
    else await page.getByRole("button", { name: s.backHome }).first().focus()
    propose(agent)
    await expect(yes(page)).toBeVisible()
    await expect(page.locator("body")).not.toBeFocused()
    await expect(card(page).locator(":focus")).toHaveCount(1)
    const shown = await page.evaluate(() => {
      const r = (document.activeElement as HTMLElement).getBoundingClientRect()
      return r.top >= 0 && r.bottom <= innerHeight
    })
    expect(shown, "the focused card title is on screen").toBe(true)
  })
}

reviewTests()
for (const viewport of [
  { width: 1180, height: 820 },
  { width: 640, height: 400 },
])
  test.describe(`at ${viewport.width}x${viewport.height}`, () => {
    test.use({ viewport })
    test.beforeEach(({ page }, info) => {
      void page
      test.skip(info.project.name !== "flow-mobile", "one extra run per size")
    })
    reviewTests()
  })

test("with the conversation hidden, the capsule is the way back and jumps to the card", async ({
  page,
  api,
}, info) => {
  test.skip(info.project.name === "flow-mobile", "the rail: tablet and desktop")
  const agent = await stockWatchInConversation(page, api)
  const hide = page.getByRole("button", { name: s.hideConversation }).first()
  if (await hide.isVisible()) await hide.click()
  propose(agent)
  // A review is a reply: the conversation shows again with the card…
  await expect(yes(page)).toBeVisible()
  // …and hidden again, the capsule appears as the way back.
  await page.getByRole("button", { name: s.hideConversation }).first().click()
  await expect(capsule(page)).toBeVisible()
  await capsule(page).getByRole("button").first().click()
  await expect(yes(page)).toBeVisible()
  await expect(page).toHaveURL(/\/scenes\/stock$/)
})

test("home's full conversation offers one way home, with or without a review", async ({
  page,
  api,
}) => {
  void api // seeds the session and the API mock
  await stockRoutes(page)
  const agent = await fakeAgent(page, (text, a) =>
    setTimeout(() => a.say(`Noted: ${text}`), 120)
  )
  await page.goto("/")
  await page.getByRole("button", { name: typeInstead, exact: true }).click()
  await page.getByLabel(v.message).fill("Which products are low?")
  await page.getByRole("button", { name: v.send, exact: true }).click()
  await expect(page.getByText("Noted: Which products are low?").first()).toBeVisible()
  const home = page.getByRole("button", { name: s.backHome }).filter({ visible: true })
  await expect(home).toHaveCount(1)
  propose(agent)
  await expect(yes(page)).toBeVisible()
  await expect(home).toHaveCount(1)
})
