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
import { s } from "./shell"

// SIMULATED voice: mock upstream behind Flow's real proxy + a fake ElevenLabs socket.

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi())
})

/** Asks for an order; the mock proposes it for review, which opens the result scene by itself. */
async function propose(page: Page) {
  const agent = await fakeAgent(page)
  const confirms = watch(page, "confirm")
  await page.goto("/")
  await startText(page)
  agent.call("orders_create", { body: { reference: "Flow e2e order" } })
  // The dock's pending row (shown while the scene is closed) and the scene with the full review.
  const pending = page.getByRole("group", { name: chat.reviewPending })
  const review = page.getByRole("region", { name: chat.results })
  await expect(review).toContainText("Flow e2e order")
  await expect(review).toContainText("Confirm action 135790")
  return { agent, confirms, pending, review, actions: review }
}

test("approving runs the action once, and only the backend's answer says it is done", async ({
  page,
}) => {
  const { confirms, actions, review } = await propose(page)
  await expectNoHorizontalScroll(page)
  // The composer cannot be used while the action waits, so nothing cancels it by accident.
  expect(
    await page
      .getByLabel(v.message)
      .evaluate((box) => box.closest("[inert]") !== null)
  ).toBe(true)
  await actions.getByRole("button", { name: v.approve }).dblclick()
  // The decision is made: the scene the review opened closes by itself, back to the conversation.
  await expect(review).toBeHidden()
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
  const { confirms, actions, review } = await propose(page)
  await actions.getByRole("button", { name: v.cancel, exact: true }).click()
  await expect(review).toBeHidden()
  await expect.poll(() => confirms.length).toBe(1)
  expect(confirms[0]!.body).toMatchObject({ decision: "cancel" })
  await expect(page.getByText("Order created.")).toHaveCount(0)
  await expect(page.getByRole("link", { name: /ORD-0043/ })).toHaveCount(0)
})

test("leaving the review keeps the action pending and reachable from the dock", async ({
  page,
}) => {
  const { confirms, review, pending } = await propose(page)
  await expect(pending).toHaveCount(0)
  await page.getByRole("button", { name: s.backHome }).click()
  await expect(review).toBeHidden()
  await expect(page.getByRole("log")).toBeVisible()
  await expect(pending).toContainText(chat.reviewPending)
  await expect(pending).toContainText("Create order for Neema Shop")
  expect(confirms).toHaveLength(0)
  await pending.getByRole("button", { name: chat.openReview }).click()
  await expect(review).toContainText("Confirm action 135790")
  await expect(pending).toHaveCount(0)
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
