// J2: a receipt says what changed in plain words, on the thing that changed; never a raw id
// ("no raw codes on screen"). SIMULATED API and voice.
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { proposeStock, readBack, receipt, scene, stockRoutes } from "./task"
import { fakeAgent, startText, v } from "./voice"

const UUID = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i

test.beforeEach(async ({ page }) => {
  await seedSession(page)
})

test("J2 a confirmed stock change reads as one sentence, with no id or reference code", async ({ page }) => {
  await mockApi(page, flowApi(readBack("149.00")))
  const agent = await fakeAgent(page)
  await stockRoutes(page)
  await proposeStock(page, agent, 150)
  await scene(page).getByRole("button", { name: v.approve }).click()
  await expect(receipt(page)).toHaveAttribute("data-receipt", "completed")
  // The server's value, read after the write: never the proposal.
  await expect(receipt(page).locator("[data-receipt-summary]")).toHaveText(
    "Raw Honey 500g in Main store is now 149 (was 14)."
  )
  await expect(receipt(page)).not.toContainText(UUID)
  await expect(receipt(page)).not.toContainText("Reference")
  // Support can still find the action: kept out of sight.
  await expect(receipt(page)).toHaveAttribute("data-reference", UUID)
  await expectNoHorizontalScroll(page)
})

test("J2 any other approved action shows no raw id either", async ({ page }) => {
  await mockApi(page, flowApi())
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  agent.call("orders_create", { body: { reference: "Flow e2e order" } })
  await scene(page).getByRole("button", { name: v.approve }).click()
  await expect(receipt(page)).toHaveAttribute("data-receipt", "completed")
  await expect(receipt(page)).not.toContainText(UUID)
  await expect(receipt(page)).not.toContainText("Reference")
})
