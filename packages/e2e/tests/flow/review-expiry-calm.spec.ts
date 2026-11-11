import { seedSession } from "../../src/session"
import { expect, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { orb } from "./shell"
import { proposeStock, readBack, stockRoutes } from "./task"
import { fakeAgent } from "./voice"

// SIMULATED voice. P8-2: a review that lapses is not an error; the orb stays calm.

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi(readBack("120.00")))
})

test("P8-2: an open review that expires says so in the dock; the orb never reads error", async ({
  page,
}) => {
  await page.clock.install()
  const agent = await fakeAgent(page)
  await stockRoutes(page)
  await proposeStock(page, agent)
  await page.clock.fastForward(11 * 60_000)
  await expect(page.getByRole("alert").first()).toContainText(
    "This review expired"
  )
  const genesy = orb(page).locator("[data-genesy-orb]")
  await expect(genesy).toHaveCount(1)
  await expect(genesy).not.toHaveAttribute("data-state", "error")
})
