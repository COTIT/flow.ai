import { seedSession } from "../../src/session"
import { expect, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { fakeAgent, startText, v } from "./voice"
import { dock, phase, s } from "./shell"

// SIMULATED voice: mock upstream behind Flow's real proxy + a fake ElevenLabs socket.

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi())
})

test("READY → IN CONVERSATION → ENDED when the person ends it", async ({ page }) => {
  await fakeAgent(page)
  await page.goto("/")
  await expect(phase(page)).toHaveText(new RegExp(s.phase.ready))
  await startText(page)
  await expect(phase(page)).toHaveText(new RegExp(s.phase.live))
  await page.getByRole("button", { name: v.end }).click()
  await expect(phase(page)).toHaveText(new RegExp(s.phase.ended))
  await expect(dock(page).getByRole("status")).toContainText(s.ended)
  await expect(dock(page).getByRole("status")).not.toContainText(s.preserved)
  // A new conversation is a new one: READY words never come back as "still connected".
  await startText(page)
  await expect(phase(page)).toHaveText(new RegExp(s.phase.live))
})

test("a dropped connection is shown as ended, honestly, with the way to continue", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  agent.close()
  await expect(phase(page)).toHaveText(new RegExp(s.phase.ended))
  await expect(dock(page).getByRole("status")).toContainText(s.preserved)
  await expect(
    dock(page).getByRole("button", { name: v.unfinished.continue })
  ).toBeVisible()
})
