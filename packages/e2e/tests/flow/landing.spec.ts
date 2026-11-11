import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { flowApi, mockApi, readiness } from "./api"

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi())
})

test("Office's real answer reaches the landing through Flow's proxy", async ({
  page,
}) => {
  // No page.route: the built proxy asks the mock standing in for Office (ready).
  await page.goto("/")
  // Available: the conversation offers its start buttons (nothing starts by itself).
  await expect(
    page.getByRole("button", { name: "Use text instead", exact: true })
  ).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test.describe("assistant switched off", () => {
  test.use({
    allow: { responses: [{ url: "/api/voice/readiness", status: 503 }] },
  })

  test("says it is turned off (Office's answer while paused)", async ({
    page,
  }) => {
    await readiness(page, 200, { enabled: false, ready: true })
    await page.goto("/")
    await expect(
      page
        .getByRole("status")
        .filter({ hasText: "The assistant is turned off" })
    ).toBeVisible()
    await expect(page.getByRole("button", { name: "Try again" })).toHaveCount(0)
    await expect(
      page.getByRole("button", { name: "Use text instead", exact: true })
    ).toHaveCount(0)
  })

  test("switched on but not ready: says so and can check again", async ({
    page,
  }) => {
    let ready = false
    await page.route("**/api/voice/readiness", (route) =>
      route.fulfill({
        status: ready ? 200 : 503,
        contentType: "application/json",
        body: JSON.stringify({ enabled: true, ready }),
      })
    )
    await page.goto("/")
    await expect(
      page.getByRole("status").filter({ hasText: "The assistant is not ready" })
    ).toBeVisible()
    await expect(
      page.getByRole("button", { name: "Use text instead", exact: true })
    ).toHaveCount(0)
    ready = true
    await page.getByRole("button", { name: "Try again" }).click()
    await expect(
      page.getByRole("button", { name: "Use text instead", exact: true })
    ).toBeVisible()
  })
})

test.describe("the check itself fails", () => {
  test.use({
    allow: { responses: [{ url: "/api/voice/readiness", status: 502 }] },
  })

  test("says it could not check, in words, with a retry", async ({ page }) => {
    await readiness(page, 502, {
      message: "The assistant service is not available right now. Try again.",
    })
    await page.goto("/")
    const notice = page
      .getByRole("alert")
      .filter({ hasText: "Could not check the assistant" })
    await expect(notice).toBeVisible()
    await expect(notice).not.toContainText("502")
    await expect(page.getByRole("button", { name: "Try again" })).toBeVisible()
  })
})
