import type { Page } from "@playwright/test"

import { seedSession } from "../../src/session"
import { expect, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { s } from "./shell"
import {
  capsule,
  proposeStock,
  readBack,
  receipt,
  scene,
  stockRoutes,
} from "./task"
import { fakeAgent, v } from "./voice"

// SIMULATED voice (fake ElevenLabs socket + Office-shaped answers); read-back is the mocked API.
// The task card (review → receipt) stays reachable on every way out and back.

const MEMORY = "genesy.flow.task"

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi(readBack("120.00")))
})

async function approved(page: Page) {
  const agent = await fakeAgent(page)
  await stockRoutes(page)
  await proposeStock(page, agent, 120)
  await expect(page).toHaveURL(/\/\?view=task$/)
  await scene(page).getByRole("button", { name: v.approve }).click()
  await expect(receipt(page).locator("[data-receipt-new]")).toHaveText("120")
}

const backHome = (page: Page) =>
  page.getByRole("button", { name: s.backHome }).first().click()

test("a review left with Back to home comes back with the browser's Back, and goes again with Forward", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  await stockRoutes(page)
  await proposeStock(page, agent, 120)
  await backHome(page)
  await expect(scene(page)).toHaveCount(0)
  await page.goBack()
  await expect(scene(page)).toContainText("Confirm action 135790")
  await page.goForward()
  await expect(scene(page)).toHaveCount(0)
  await expect(capsule(page)).toHaveAttribute(
    "data-task-state",
    "needs_approval"
  )
})

test("an approved card: home, then Back, shows its receipt again", async ({
  page,
}) => {
  await approved(page)
  await backHome(page)
  await expect(receipt(page)).toHaveCount(0)
  await page.goBack()
  await expect(receipt(page).locator("[data-receipt-new]")).toHaveText("120")
})

test("the receipt survives a reload of the tab; its memory holds no confirmation code or token", async ({
  page,
}) => {
  await approved(page)
  await page.reload()
  await expect(receipt(page).locator("[data-receipt-new]")).toHaveText("120")
  await backHome(page)
  await expect(capsule(page)).toHaveAttribute("data-task-state", "completed")
  const saved = await page.evaluate(
    (key) => sessionStorage.getItem(key) ?? "",
    MEMORY
  )
  const token = await page.evaluate(() => JSON.stringify(localStorage))
  expect(saved).toContain("Raw Honey 500g")
  expect(saved).not.toContain("135790")
  for (const secret of token.match(/[A-Za-z0-9_-]{24,}/g) ?? [])
    expect(saved, "the login token is never stored").not.toContain(secret)
})

test("another login's or workspace's memory, or a broken one, is never shown and is dropped", async ({
  page,
}) => {
  await approved(page)
  const saved = await page.evaluate(
    (key) => sessionStorage.getItem(key)!,
    MEMORY
  )
  for (const foreign of [
    saved.replace(/"owner":"[^"]+"/, '"owner":"someone-else"'),
    "{not json",
  ]) {
    await page.evaluate(
      ([key, value]) => sessionStorage.setItem(key!, value!),
      [MEMORY, foreign]
    )
    await page.reload()
    await expect(
      page
        .getByRole("heading", { name: /results/i })
        .or(page.getByRole("button", { name: v.textInstead, exact: true }))
        .first()
    ).toBeVisible()
    await expect(receipt(page)).toHaveCount(0)
    await expect(capsule(page)).toHaveCount(0)
    expect(
      await page.evaluate((key) => sessionStorage.getItem(key), MEMORY)
    ).not.toBe(foreign)
  }
})

test("a reload while a review is open never lands on an empty task scene", async ({ page }) => {
  const agent = await fakeAgent(page)
  await stockRoutes(page)
  await proposeStock(page, agent, 120)
  await page.reload()
  // Nothing of that conversation is left in this tab: home, never an empty "Results" scene.
  await expect(page.getByRole("button", { name: v.textInstead, exact: true })).toBeVisible()
  await expect(scene(page)).toHaveCount(0)
  await expect(page).toHaveURL(/\/$/)
})
