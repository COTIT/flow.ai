import { seedSession } from "../../src/session"
import { expect, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import {
  capsule,
  k,
  proposeStock,
  readBack,
  receipt,
  scene,
  stockReview,
  stockRoutes,
} from "./task"
import { fakeAgent, startText, v } from "./voice"

// SIMULATED voice; receipts must come from the server's answer or a read after the write.

test.beforeEach(async ({ page }) => {
  await seedSession(page)
})

test("F: a read-back that disagrees with the proposal shows the server's value", async ({
  page,
}) => {
  await mockApi(page, flowApi(readBack("149.00")))
  const agent = await fakeAgent(page)
  await stockRoutes(page)
  await proposeStock(page, agent, 150)
  await scene(page).getByRole("button", { name: v.approve }).click()
  await expect(receipt(page).locator("[data-receipt-new]")).toHaveText("149")
  // The action id is kept for support, never shown (J2).
  await expect(receipt(page)).toHaveAttribute("data-reference", /^0193f6b0-/)
})

test("P6-5: an unconfirmed outcome shows what was approved, never a blank new value", async ({
  page,
}) => {
  await mockApi(page, flowApi(readBack("149.00")))
  const agent = await fakeAgent(page)
  await stockRoutes(page, () => ({ status: "unknown", data: {} }))
  await proposeStock(page, agent, 150)
  await scene(page).getByRole("button", { name: v.approve }).click()
  await expect(receipt(page)).toHaveAttribute("data-receipt", "unknown")
  // Labelled as the approval, not as a value the server confirmed (no read-back ran).
  await expect(receipt(page).locator("[data-receipt-new]")).toHaveText(
    k.receipt.approved.replace("{value}", "150")
  )
  await expect(receipt(page)).not.toContainText("149")
})

test.describe("the read-back fails", () => {
  test.use({
    allow: { responses: [{ url: "/v1/workspace/w1/products/p-500/stock-with-bins", status: 503 }] },
  })

  test("says it completed but the value could not be loaded; Refresh reads again", async ({
    page,
  }) => {
    let fail = true
    await mockApi(page, flowApi())
    await page.route("**/v1/workspace/w1/products/p-500/stock-with-bins", (route) =>
      fail
        ? route.fulfill({ status: 503, json: { message: "down" } })
        : route.fulfill({ json: readBack("150.00")[
            "GET /workspace/w1/products/p-500/stock-with-bins"
          ]!.body })
    )
    const agent = await fakeAgent(page)
    await stockRoutes(page)
    await proposeStock(page, agent, 150)
    await scene(page).getByRole("button", { name: v.approve }).click()
    const value = receipt(page).locator("[data-receipt-new]")
    // Never the assumed (proposed) value.
    await expect(value).toContainText(k.receipt.readFailed, { timeout: 15_000 })
    await expect(value).not.toContainText("150")
    fail = false
    await value.getByRole("button", { name: k.receipt.refresh }).click()
    await expect(value).toHaveText("150")
  })
})

test("a stock change shows the API's own before and after", async ({ page }) => {
  await mockApi(page, flowApi())
  const agent = await fakeAgent(page)
  await stockRoutes(page)
  await page.route("**/api/voice/tools", async (route, request) => {
    const b = request.postDataJSON() as { operationId?: string }
    if (b.operationId !== "stock_mutate") return route.fallback()
    const review = stockReview(0, "mutate-20").review
    return route.fulfill({
      json: {
        status: "requires_confirmation",
        data: { actionId: "0193f6b0-1c2d-7e3f-8a9b-000000000020" },
        review: { ...review, operationId: "stock_mutate", details: { record: review.details.record } },
      },
    })
  })
  await page.route("**/api/voice/confirm", (route) =>
    route.fulfill({
      json: {
        status: "completed",
        actionId: "0193f6b0-1c2d-7e3f-8a9b-000000000020",
        data: {
          message: "ok",
          before: "14.00",
          after: "34.00",
          warehouse: { id: "wh-main", name: "Main store" },
        },
      },
    })
  )
  await page.goto("/")
  await startText(page)
  agent.call("stock_mutate", { path: { product: "p-500" }, body: { mutation_value: 20 } })
  await scene(page).getByRole("button", { name: v.approve }).click()
  await expect(receipt(page).locator("[data-receipt-new]")).toHaveText("34")
  await expect(receipt(page).locator("[data-receipt-row]")).toContainText("14")
})

test("a refused write is 'not done', with the reason in plain words and no new value", async ({
  page,
}) => {
  await mockApi(page, flowApi(readBack("150.00")))
  const agent = await fakeAgent(page)
  await stockRoutes(page, () => ({
    status: "failed",
    reason: "forbidden",
    actionId: "0193f6b0-1c2d-7e3f-8a9b-000000000150",
  }))
  await proposeStock(page, agent, 150)
  await scene(page).getByRole("button", { name: v.approve }).click()
  await expect(receipt(page)).toHaveAttribute("data-receipt", "failed")
  await expect(receipt(page)).toContainText(v.requestErrors.forbidden)
  await expect(receipt(page).locator("[data-receipt-new]")).toHaveText("—")
})

test("a read that does not contain the changed slot says it could not confirm; never 0", async ({
  page,
}) => {
  // Only a lot row in that warehouse: the slot the review named is not in the read.
  const lotOnly = readBack("150.00")
  const reply = lotOnly["GET /workspace/w1/products/p-500/stock-with-bins"]!
  ;(reply.body as { data: { lot: unknown }[] }).data[0]!.lot = { id: "l1", code: "L1" }
  await mockApi(page, flowApi(lotOnly))
  const agent = await fakeAgent(page)
  await stockRoutes(page)
  await proposeStock(page, agent, 150)
  await scene(page).getByRole("button", { name: v.approve }).click()
  const value = receipt(page).locator("[data-receipt-new]")
  await expect(value).toContainText(k.receipt.unconfirmed)
  await expect(value.getByRole("button", { name: k.receipt.refresh })).toBeVisible()
  await expect(value).not.toContainText(/\b(0|150)\b/)
  await expect(receipt(page)).toHaveAttribute("data-receipt", "completed")
})

test.describe("a later read fails", () => {
  test.use({
    allow: {
      responses: [{ url: "/v1/workspace/w1/products/p-500/stock-with-bins", status: 503 }],
    },
  })

  test("the earlier value is dropped, not shown as current", async ({ page }) => {
    let fail = false
    await mockApi(page, flowApi())
    await page.route("**/v1/workspace/w1/products/p-500/stock-with-bins", (route) =>
      fail
        ? route.fulfill({ status: 503, json: { message: "down" } })
        : route.fulfill({
            json: readBack("150.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!.body,
          })
    )
    const agent = await fakeAgent(page)
    await stockRoutes(page)
    await proposeStock(page, agent, 150)
    await scene(page).getByRole("button", { name: v.approve }).click()
    const value = receipt(page).locator("[data-receipt-new]")
    await expect(value).toHaveText("150")
    // Close and reopen the receipt: it reads again, and that read fails.
    fail = true
    await page.getByRole("button", { name: "Back to home" }).click()
    await capsule(page).getByRole("button", { name: k.openReceipt }).click()
    await expect(value).toContainText(k.receipt.readFailed, { timeout: 15_000 })
    await expect(value).not.toContainText("150")
  })
})
