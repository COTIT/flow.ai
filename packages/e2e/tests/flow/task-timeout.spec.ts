import { seedSession } from "../../src/session"
import { expect, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { phase } from "./shell"
import {
  ACTION,
  k,
  proposeStock,
  readBack,
  receipt,
  scene,
  stockReview,
  stockRoutes,
} from "./task"
import { fakeAgent, v } from "./voice"

// SIMULATED voice: the provider ends the socket; Office's recovery answers come from the routes.

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi(readBack("120.00")))
})

test("D: the voice session ends during a review; the task is kept and resumes visually, no new voice", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  const confirms = await stockRoutes(page)
  const commands: Record<string, unknown>[] = []
  await page.route("**/api/voice/tools", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>
    if (body.command === "status") {
      commands.push(body)
      return route.fulfill({
        json: {
          status: "completed",
          data: [
            {
              status: "awaiting_review",
              data: {
                actionId: ACTION(120),
                operation: "stock_set_levels",
                summary: "Set stock levels per warehouse",
                recoverable: true,
              },
            },
          ],
        },
      })
    }
    if (body.command === "restore") {
      commands.push(body)
      return route.fulfill({ json: stockReview(120, "stock-120-restored") })
    }
    return route.fallback()
  })
  await proposeStock(page, agent, 120)
  agent.close()

  // Reported accurately: ended, the work preserved, a resume prompt from the recorded task.
  await expect(phase(page)).toHaveAttribute("data-phase", "ended")
  const prompt = page.getByRole("region", { name: k.resume.title })
  await expect(prompt).toContainText("Set stock levels per warehouse")
  await expect(scene(page)).toHaveCount(0)
  expect(confirms).toHaveLength(0)

  await prompt.getByRole("button", { name: k.resume.continue }).click()
  // A recovered review is approved by its button only (no spoken phrase without a conversation).
  await expect(scene(page)).toContainText(v.recoveredReviewHint)
  expect(commands.map((c) => c.command)).toEqual(["status", "restore"])
  expect(commands[1]).toMatchObject({ actionId: ACTION(120), sessionId: "flow-session" })
  // Resumed visually: no new voice connection.
  expect(agent.connections).toBe(1)

  await scene(page).getByRole("button", { name: v.approve }).click()
  await expect(receipt(page).locator("[data-receipt-new]")).toHaveText("120")
  expect(confirms).toEqual([
    expect.objectContaining({ decision: "approve", ticket: "stock-120-restored" }),
  ])
  expect(agent.connections).toBe(1)
})
