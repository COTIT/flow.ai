import { mockApi } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { officeApi } from "./api"

test.beforeEach(async ({ page }) => {
  await seedSession(page)
})

test("lists the workspace's products", async ({ page }) => {
  await mockApi(page, officeApi())
  await page.goto("/")
  await expect(page.getByRole("img", { name: "Genesy" })).toBeVisible()
  await expect(page.getByRole("heading", { name: "Products" })).toBeVisible()
  await expect(page.getByRole("cell", { name: "Beeswax Block" })).toBeVisible()
  await expect(page.getByRole("cell", { name: "14.99" })).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test("says so when there are no products", async ({ page }) => {
  await mockApi(page, officeApi([]))
  await page.goto("/")
  await expect(page.getByText("No products yet.")).toBeVisible()
})

test.describe("API error", () => {
  test.use({
    allow: { responses: [{ url: "/v1/workspace/w1/products", status: 500 }] },
  })

  test("shows a message instead of the table", async ({ page }) => {
    await mockApi(page, {
      ...officeApi(),
      "GET /workspace/w1/products": { status: 500, body: { message: "boom" } },
    })
    await page.goto("/")
    // Waits out TanStack Query's three retries.
    await expect(
      page
        .getByRole("alert")
        .filter({ hasText: "Something went wrong. Try again." })
    ).toBeVisible({ timeout: 15_000 })
  })
})
