import { expect, test } from "./storefront-api"
import { capsule, k, scene } from "./task"
import { fakeAgent, startText, v } from "./voice"

// SIMULATED voice and batch answers (task-supplier's shapes). P8-5: a paused batch says
// "Paused" on the task line, never "Working".

const steps = (done: number, total = 21) =>
  Array.from({ length: total }, (_, i) => ({
    status: i < done ? "completed" : "not_started",
    data: { actionId: `a-${i}`, operation: "products_update", summary: `Price ${i}` },
  }))
const batchState = (state: string, done: number) => ({
  status: "partial",
  data: { kind: "batch", batchId: "b-1", state, nextIndex: done, recoverable: false, steps: steps(done) },
})

test("P8-5: a paused batch shows its own state on the task line", async ({ page, api }) => {
  void api // seeds the session and the API mock
  const agent = await fakeAgent(page)
  let release: () => void = () => {}
  const paid = new Promise<void>((resolve) => (release = resolve))
  await page.route("**/api/voice/batch", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>
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
            details: { notIncluded: [] },
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
    if (body.command === "status") return route.fulfill({ json: batchState("running", 4) })
    if (body.command === "pause") {
      release()
      return route.fulfill({ json: batchState("paused", 8) })
    }
    return route.fallback()
  })
  await page.route("**/api/voice/confirm", async (route) => {
    await paid // still running when Pause is pressed
    return route.fulfill({ json: batchState("paused", 8) })
  })

  await page.goto("/")
  await startText(page)
  agent.call("assistant_batch_prepare", { calls: [{}, {}] })
  await scene(page).getByRole("button", { name: v.approve }).click()
  await scene(page).getByRole("button", { name: k.pause }).click()
  await expect(capsule(page)).toHaveAttribute("data-task-state", "paused")
  await expect(capsule(page)).toContainText(/^Paused/i)
  await expect(capsule(page)).not.toContainText("Working", { ignoreCase: true })
})
