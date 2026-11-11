import type { Page } from "@playwright/test"

import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import {
  chat,
  fakeAgent,
  FLOW_ORDER,
  OFFICE,
  startText,
  v,
  watch,
} from "./voice"

// SIMULATED voice: mock upstream behind Flow's real proxy + a fake ElevenLabs socket.

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi())
})

const narrow = (page: Page) => page.viewportSize()!.width < 1024

/** Asks for an order; the mock proposes it for review. Returns where the review's buttons are. */
async function propose(page: Page) {
  const agent = await fakeAgent(page)
  const confirms = watch(page, "confirm")
  await page.goto("/")
  await startText(page)
  agent.call("orders_create", { body: { reference: "Flow e2e order" } })
  const pending = page.getByRole("group", { name: chat.reviewPending })
  // Small screens: the sheet opens by itself (modal, so the page behind it is hidden), as Office's review does.
  if (!narrow(page)) await expect(pending).toContainText(chat.reviewPending)
  const review = narrow(page)
    ? page.getByRole("dialog", { name: chat.results })
    : page.getByRole("region", { name: chat.results })
  await expect(review).toContainText("Flow e2e order")
  await expect(review).toContainText("Confirm action 135790")
  const actions = narrow(page) ? review : pending
  return { agent, confirms, pending, review, actions }
}

test("approving runs the action once, and only the backend's answer says it is done", async ({
  page,
}) => {
  const { confirms, actions, pending } = await propose(page)
  await expectNoHorizontalScroll(page)
  // The composer cannot be used while the action waits, so nothing cancels it by accident.
  expect(
    await page
      .getByLabel(v.message)
      .evaluate((box) => box.closest("[inert]") !== null)
  ).toBe(true)
  await actions.getByRole("button", { name: v.approve }).dblclick()
  await expect(pending).toBeHidden()
  // The decision is made: the review sheet closes by itself on small screens.
  await expect(page.getByRole("dialog")).toHaveCount(0)
  await expect
    .poll(() => confirms.filter((c) => c.method === "POST").length)
    .toBe(1)
  expect(confirms[0]!.body).toMatchObject({
    decision: "approve",
    method: "click",
    ticket: "flow-review-ticket",
  })
  const link = page.getByRole("log").getByRole("link", { name: /ORD-0043/ })
  await expect(link).toHaveAttribute("href", `${OFFICE}/orders/${FLOW_ORDER}`)
  await expect(
    page.getByRole("status").filter({ hasText: v.activity.completed })
  ).toBeVisible()
  expect(confirms).toHaveLength(1)
})

test("rejecting sends a cancel and changes nothing", async ({ page }) => {
  const { confirms, actions, pending } = await propose(page)
  await actions.getByRole("button", { name: v.cancel, exact: true }).click()
  await expect(pending).toBeHidden()
  await expect.poll(() => confirms.length).toBe(1)
  expect(confirms[0]!.body).toMatchObject({ decision: "cancel" })
  await expect(page.getByText("Order created.")).toHaveCount(0)
  await expect(page.getByRole("link", { name: /ORD-0043/ })).toHaveCount(0)
})

test("on a small screen, closing the review sheet keeps the action pending and reachable", async ({
  page,
}) => {
  test.skip(
    page.viewportSize()!.width >= 1024,
    "the sheet exists only below 1024 px"
  )
  const { confirms, review, pending } = await propose(page)
  await page.getByRole("button", { name: chat.closeResults }).click()
  await expect(review).toBeHidden()
  await expect(pending).toContainText(chat.reviewPending)
  await expect(pending).toContainText("Create order for Neema Shop")
  expect(confirms).toHaveLength(0)
  await pending.getByRole("button", { name: chat.openReview }).click()
  await expect(review).toBeVisible()
  await page.keyboard.press("Escape")
  await expect(review).toBeHidden()
  await expect(pending).toBeVisible()
  expect(confirms).toHaveLength(0)
})

test.describe("the backend refuses the approval", () => {
  test.use({
    allow: { responses: [{ url: "/api/voice/confirm", status: 409 }] },
  })

  test("says so in plain words and never claims success", async ({ page }) => {
    await page.route("**/api/voice/confirm", (route) =>
      route.fulfill({
        status: 409,
        json: { message: "Raw backend detail that must stay off screen" },
      })
    )
    const { actions } = await propose(page)
    await actions.getByRole("button", { name: v.approve }).click()
    await expect(page.getByRole("alert").first()).toContainText(
      v.requestErrors.conflict
    )
    await expect(
      page.getByText("Raw backend detail that must stay off screen")
    ).toHaveCount(0)
    await expect(page.getByRole("link", { name: /ORD-0043/ })).toHaveCount(0)
    await expect(
      page.getByRole("status").filter({ hasText: v.activity.completed })
    ).toHaveCount(0)
  })
})

test.describe("the approval's answer is lost", () => {
  test.use({ allow: { failures: ["/api/voice/confirm"] } })

  test("says the outcome needs checking, not that it worked", async ({
    page,
  }) => {
    await page.route("**/api/voice/confirm", (route) => route.abort("failed"))
    const { actions } = await propose(page)
    await actions.getByRole("button", { name: v.approve }).click()
    await expect(
      page.getByRole("status").filter({ hasText: v.activity.unknown })
    ).toBeVisible()
    await expect(page.getByRole("link", { name: /ORD-0043/ })).toHaveCount(0)
  })
})
