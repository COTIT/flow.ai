import { seedSession } from "../../src/session"
import { expect, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { backToCard, k, proposeStock, readBack, scene, stockRoutes, taskLine } from "./task"
import { fakeAgent, watch } from "./voice"

// SIMULATED voice: the fake socket sends ElevenLabs' interruption and transcript events.

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi(readBack("150.00")))
})

test("B: an interruption alone cancels nothing; the correction withdraws the proposal; same session", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  const confirms = await stockRoutes(page)
  const sessions = watch(page, "session")
  await proposeStock(page, agent, 120)
  agent.say("I found Raw Honey 500g, and the current—")
  agent.emit({ type: "interruption", interruption_event: { event_id: 3 } })
  // Being interrupted is not a decision: the review is still open, nothing was sent.
  await expect(await taskLine(page)).toHaveAttribute("data-task-state", "needs_approval")
  expect(confirms).toHaveLength(0)
  await backToCard(page)

  agent.emit({
    type: "user_transcript",
    user_transcription_event: { user_transcript: "Actually, make that 150.", event_id: 4 },
  })
  await expect.poll(() => confirms.length).toBe(1)
  expect(confirms[0]).toMatchObject({ decision: "cancel", ticket: "stock-120" })
  // The cancelled review closes once the cancel is through (P5: its withdrawn note stays in place).
  await expect(page.locator("#flow-review-title")).toHaveCount(0)

  // The same conversation carries on: the new proposal arrives on the same socket.
  agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 150 }] },
  })
  await expect(scene(page)).toContainText("Confirm action 135790")
  // Above the new review: the replaced one, never sent.
  await expect(page.locator("[data-withdrawn]")).toContainText(k.withdrawn)
  await expect(page.locator("[data-withdrawn]")).toContainText("→ 120")
  await expect(await taskLine(page)).toHaveAttribute("data-task-state", "needs_approval")
  expect(agent.connections).toBe(1)
  expect(sessions.filter((s) => s.method === "POST")).toHaveLength(1)
  expect(confirms.filter((c) => c.decision === "approve")).toHaveLength(0)
})
