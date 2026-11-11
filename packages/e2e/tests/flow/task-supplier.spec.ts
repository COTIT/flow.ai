import { expect, test, designs } from "./storefront-api"
import { capsule, k, scene } from "./task"
import { fakeAgent, startText, v } from "./voice"

// SIMULATED voice and batch answers (Office's batch shapes). The price sheet itself is not
// uploaded here (attachments have their own spec): the batch is the review Office would build.

const steps = (done: number, total = 21) =>
  Array.from({ length: total }, (_, i) => ({
    status: i < done ? "completed" : "not_started",
    data: { actionId: `a-${i}`, operation: "products_update", summary: `Price ${i}` },
  }))
const paused = {
  status: "partial",
  data: { kind: "batch", batchId: "b-1", state: "paused", nextIndex: 8, recoverable: false, steps: steps(8) },
}

test("C: supplier prices: 21 matched, 3 exceptions kept out; leave and return; pause; the rest needs a new review", async ({
  page,
  api,
}) => {
  designs(api)
  const agent = await fakeAgent(page)
  const batch: Record<string, unknown>[] = []
  const confirms: Record<string, unknown>[] = []
  let release: () => void = () => {}
  const paid = new Promise<void>((resolve) => (release = resolve))
  await page.route("**/api/voice/batch", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>
    batch.push(body)
    if (body.command === "prepare")
      return route.fulfill({
        json: {
          status: "requires_confirmation",
          data: { batchId: "b-1" },
          review: {
            ticket: "batch-21",
            kind: "batch",
            batchId: "b-1",
            summary: "Update 21 supplier purchase prices",
            confirmationCode: "246810",
            confirmationPhrase: "Confirm batch 246810",
            details: {
              notIncluded: [
                { line: 4, text: "Raw Honey 2kg", reason: "not_found" },
                { line: 9, text: "Beeswax", reason: "ambiguous" },
                { line: 17, text: "Tea Organic", reason: "unclear_precision" },
              ],
            },
            steps: steps(0).map((s) => ({
              actionId: s.data.actionId,
              operationId: "products_update",
              summary: s.data.summary,
              details: {},
            })),
            expiresAt: new Date(Date.now() + 10 * 60_000).toISOString(),
          },
        },
      })
    if (body.command === "pause") {
      release()
      return route.fulfill({ json: paused })
    }
    return route.fallback()
  })
  await page.route("**/api/voice/confirm", async (route) => {
    confirms.push(route.request().postDataJSON() as Record<string, unknown>)
    await paid // the run is still going when Pause is pressed
    return route.fulfill({ json: paused })
  })

  await page.goto("/")
  await startText(page)
  agent.call("assistant_batch_prepare", { calls: [{}, {}] })
  const summary = page.locator("[data-batch]")
  await expect(summary).toContainText("21 changes matched")
  await expect(summary).toContainText("3 exceptions not included")

  // Leave for another scene: the task stays reachable and waits; nothing runs.
  await page.getByRole("link", { name: "Storefront" }).click()
  await expect(page).toHaveURL(/\/storefront$/)
  await expect(capsule(page)).toHaveAttribute("data-task-state", "needs_approval")
  await capsule(page).getByRole("button", { name: k.review }).click()
  await expect(page).toHaveURL(/\/$/)
  await expect(summary).toContainText("21 changes matched")
  expect(confirms).toHaveLength(0)

  // Approve, then pause while it runs: done steps stay done, the rest does not run.
  await scene(page).getByRole("button", { name: v.approve }).click()
  await scene(page).getByRole("button", { name: k.pause }).click()
  await expect(summary).toHaveAttribute("data-batch", "paused")
  await expect(summary).toContainText("8 updated · 0 failed · 13 not run · 0 need checking")
  await expect(page.getByText(/24 (updated|changes)/)).toHaveCount(0)
  expect(batch.map((b) => b.command)).toEqual(["prepare", "pause"])

  // Continue asks Genesy for a NEW review of what did not run; nothing resumes by itself.
  await summary.getByRole("button", { name: k.batch.continue }).click()
  await expect
    .poll(() =>
      agent.sent.some(
        (e) =>
          e.type === "user_message" &&
          e.text === "Prepare a new review for the 13 changes that did not run."
      )
    )
    .toBe(true)
  expect(confirms).toHaveLength(1)
  expect(agent.connections).toBe(1)
})
