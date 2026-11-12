import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { readBack, scene, stockReview } from "./task"
import { fakeAgent, startText, v } from "./voice"

// SIMULATED voice (fake socket + Office-shaped review); the review rows are the shape the API's
// stock facts now carry (`current.changes`). P3-3: the amount being set leads the card.

const diff = (page: Parameters<typeof scene>[0]) => scene(page).locator("[data-review-diff]")

async function review(page: Parameters<typeof scene>[0], operationId: string, answer: object) {
  await page.route("**/api/voice/tools", async (route) => {
    const sent = route.request().postDataJSON() as { command?: string; operationId?: string }
    if (sent.command === "execute" && sent.operationId === operationId)
      return route.fulfill({ json: answer })
    return route.fallback()
  })
}

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi(readBack("14.00")))
})

test("P3-3: a stock review shows the warehouse, 14 → 130 and a plain title before More details", async ({
  page,
}) => {
  const stock = stockReview(130)
  stock.review.details.current = {
    ...stock.review.details.current,
    changes: [{ field: "stock", label: "Main store", before: "14", after: "130" }],
  } as typeof stock.review.details.current
  await review(page, "stock_set_levels", stock)
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  agent.call("stock_set_levels", {
    path: { product: "p-500" },
    body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 130 }] },
  })
  await expect(scene(page)).toContainText(v.operationTitles.stock_set_levels)
  await expect(scene(page)).not.toContainText("Run action")
  await expect(diff(page)).toContainText(`Main store · ${v.reviewFields.stock}`)
  await expect(diff(page).locator("del")).toHaveText(/14/)
  await expect(diff(page).locator("ins")).toHaveText(/130/)
  // Folded until asked: the numbers above never need it.
  await expect(scene(page).getByRole("button", { name: v.results.moreDetails })).toHaveAttribute(
    "aria-expanded",
    "false"
  )
  await expect(scene(page).getByRole("button", { name: v.approve })).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test("without API rows the request's plain values lead; ids stay folded away", async ({ page }) => {
  const update = {
    status: "requires_confirmation",
    data: { actionId: "0193f6b0-1c2d-7e3f-8a9b-000000000777" },
    review: {
      ticket: "relation-777",
      operationId: "relations_update",
      method: "POST",
      summary: "Update a customer",
      confirmationCode: "135790",
      confirmationPhrase: "Confirm action 135790",
      target: "relations",
      details: {
        target: { relation: "01950000-0000-7000-8000-000000000008" },
        record: { type: "customer", label: "Kendall Moreau" },
        changes: {
          company_name: "Mama Nzuri Ltd",
          country_id: "01950000-0000-7000-8000-000000000009",
        },
      },
      expiresAt: new Date(Date.now() + 10 * 60_000).toISOString(),
    },
  }
  await review(page, "relations_update", update)
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  agent.call("relations_update", { path: { relation: "01950000-0000-7000-8000-000000000008" } })
  await expect(diff(page)).toContainText(v.reviewFields.company_name)
  await expect(diff(page).locator("ins")).toHaveText(/Mama Nzuri Ltd/)
  await expect(diff(page).locator("del")).toHaveCount(0)
  await expect(scene(page)).not.toContainText("01950000")
  await expectNoHorizontalScroll(page)
})
