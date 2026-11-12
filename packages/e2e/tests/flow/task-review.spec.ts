import type { Page } from "@playwright/test"

import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import {
  fakeAgent,
  FLOW_ORDER,
  OFFICE,
  startText,
  v,
  watch,
} from "./voice"
import { s } from "./shell"
import { backToCard, k, receipt, scene, taskLine } from "./task"

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
  // The full review, in place in the conversation (P5).
  const review = scene(page)
  await expect(review).toContainText("Flow e2e order")
  await expect(review).toContainText("Confirm action 135790")
  return { agent, confirms, review, actions: review }
}

test("approving runs the action once, and only the backend's answer says it is done", async ({
  page,
}) => {
  const { confirms, actions, review } = await propose(page)
  await expectNoHorizontalScroll(page)
  // Nothing in the dock sends or acts while the action waits, so nothing cancels it by accident
  // (the field keeps focus and draft, SW9).
  await expect(page.getByRole("button", { name: v.send, exact: true })).toBeDisabled()
  await expect(page.getByRole("button", { name: v.addFiles })).toBeDisabled()
  await actions.getByRole("button", { name: v.approve }).dblclick()
  // The decision is made: in the same scene the approval becomes its receipt, from the server's answer.
  await expect(actions.getByRole("button", { name: v.approve })).toHaveCount(0)
  await expect(receipt(page)).toHaveAttribute("data-receipt", "completed")
  await expect(receipt(page)).toContainText(k.receipt.title.completed)
  await expect(receipt(page)).toContainText(k.receipt.statuses.completed)
  // The audit reference (the assistant action id) is kept out of sight: no raw codes (J2).
  await expect(receipt(page)).toHaveAttribute("data-reference", "0193f6b0-1c2d-7e3f-8a9b-aaaaaaaaaaaa")
  await expect(receipt(page)).not.toContainText("0193f6b0")
  // Back to home (J4), then on to the conversation, where the record link and the outcome
  // line are.
  await page.getByRole("button", { name: s.backHome }).click()
  await expect(review).toBeHidden()
  await page.getByText(s.continueConversation, { exact: true }).click()
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
  // The conversation's activity line and the task line both say completed.
  await expect(
    page.locator('[role="status"][data-state="completed"]')
  ).toContainText(v.activity.completed)
  await expect(await taskLine(page)).toHaveAttribute("data-task-state", "completed")
  expect(confirms).toHaveLength(1)
})

test("rejecting sends a cancel and changes nothing", async ({ page }) => {
  const { confirms, actions, review } = await propose(page)
  await actions.getByRole("button", { name: v.cancel, exact: true }).click()
  // The review goes (P5: in place, a "never sent" note stays where it was).
  await expect(page.locator("#flow-review-title")).toHaveCount(0)
  void review
  await expect.poll(() => confirms.length).toBe(1)
  expect(confirms[0]!.body).toMatchObject({ decision: "cancel" })
  await expect(page.getByText("Order created.")).toHaveCount(0)
  await expect(page.getByRole("link", { name: /ORD-0043/ })).toHaveCount(0)
})

test("leaving the review keeps the action pending and reachable from the dock", async ({
  page,
}) => {
  const { confirms, review } = await propose(page)
  // P5: leaving the conversation (Back to home) leaves the task line on the dock as the way back.
  const pending = await taskLine(page)
  await expect(pending).toHaveAttribute("data-task-state", "needs_approval")
  await expect(review).toBeHidden()
  await expect(pending).toContainText(k.milestones.waitingConfirmation)
  await expect(pending).toContainText("Create order for Neema Shop")
  expect(confirms).toHaveLength(0)
  await backToCard(page)
  await expect(review).toContainText("Confirm action 135790")
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
    // Not done for sure, so never "confirmed": the task checks the recorded outcome.
    await expect(receipt(page)).toHaveAttribute("data-receipt", "unknown")
    await expect(page.getByText(k.receipt.title.completed)).toHaveCount(0)
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
    await expect(receipt(page)).toHaveAttribute("data-receipt", "unknown")
    await expect(receipt(page)).toContainText(k.receipt.title.unknown)
    await expect(receipt(page)).toContainText(k.receipt.unknownText)
    // Never a repeat of the write: only a status question to Genesy.
    await expect(page.getByRole("button", { name: v.approve })).toHaveCount(0)
    await expect(page.getByRole("link", { name: /ORD-0043/ })).toHaveCount(0)
  })
})
