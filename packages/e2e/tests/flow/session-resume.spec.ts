import type { Page } from "@playwright/test"

import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { sc } from "./scenes-api"
import { ACTION, k, proposeStock, readBack, scene, stockReview, stockRoutes } from "./task"
import { fakeAgent, v } from "./voice"

// The login expires while the person is away (persona P8, the idle returner). SIMULATED API and
// voice: after `expire`, the one call each test makes is refused with 401, as the API and Office
// refuse a token past its expiry; logging in again issues a NEW token for the same person.

const ENDED = "Your session ended. Log in again to carry on."
const DRAFT = "Half a thought about honey"

/** The server's view of the login: refuses `refused` (method + path) with 401 until a new login. */
async function server(page: Page, refused: string[]) {
  let expired = false
  await page.route(/\/(v1|api\/voice)\//, (route) => {
    const request = route.request()
    const path = new URL(request.url()).pathname
    if (path === "/v1/login" && request.method() === "POST") {
      expired = false
      return route.fulfill({
        json: {
          data: {
            access_token: "e2e-token-renewed",
            token_type: "Bearer",
            expires_at: "2099-01-01T00:00:00Z",
            workspace_ids: ["w1"],
          },
        },
      })
    }
    if (expired && refused.includes(`${request.method()} ${path}`))
      return route.fulfill({ status: 401, json: { message: "Unauthenticated." } })
    return route.fallback()
  })
  return { expire: () => (expired = true) }
}

async function logInAgain(page: Page) {
  await expect(page).toHaveURL(/\/login(\?|$)/)
  await expect(page.getByText(ENDED)).toBeVisible()
  await page.getByLabel("Email").fill("e2e@genesy.test")
  await page.getByLabel("Password").fill("e2e-password-only")
  await page.getByRole("button", { name: "Log in" }).click()
}

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi(readBack("120.00")))
})

test.describe("starting a conversation after the login expired", () => {
  test.use({ allow: { responses: [{ url: "/api/voice/session", status: 401 }] } })

  test("P8-3 logging in again goes back to the scene the person was on", async ({ page }) => {
    await fakeAgent(page)
    const { expire } = await server(page, ["POST /api/voice/session"])
    await page.goto("/")
    await page.getByRole("region", { name: sc.yourFlow }).getByRole("link").first().click()
    await expect(page).toHaveURL(/\/scenes\//)
    const where = new URL(page.url()).pathname
    expire()
    await page.getByLabel(v.message).fill("Show this month")
    await page.getByLabel(v.message).press("Enter")
    await logInAgain(page)
    await expect(page).toHaveURL(new RegExp(`${where}$`))
    await expectNoHorizontalScroll(page)
  })

  test("P8-1 a half-typed draft is back in the box after logging in again", async ({ page }) => {
    await fakeAgent(page)
    const { expire } = await server(page, ["POST /api/voice/session"])
    await page.goto("/")
    await page.getByLabel(v.message).fill(DRAFT)
    expire()
    await page.getByLabel(v.message).press("Enter")
    await logInAgain(page)
    await expect(page).toHaveURL(/\/$/)
    await expect(page.getByLabel(v.message)).toHaveValue(DRAFT)
  })

  test("P8-1 a draft is not given to another person who logs in on the same tab", async ({ page }) => {
    await fakeAgent(page)
    const { expire } = await server(page, ["POST /api/voice/session"])
    await page.goto("/")
    await page.getByLabel(v.message).fill(DRAFT)
    expire()
    await page.getByLabel(v.message).press("Enter")
    // Someone else logs in: /me now names another user.
    await page.route("**/v1/me", (route) =>
      route.fulfill({ json: { data: { user: { id: "u-other", first_name: "Other", permissions: [] } } } })
    )
    await logInAgain(page)
    await expect(page).toHaveURL(/\/$/)
    await expect(page.getByLabel(v.message)).toHaveValue("")
  })
})

test("P8-1 a draft is dropped when the person logs out by choice", async ({ page }) => {
  await page.goto("/")
  await page.getByLabel(v.message).fill(DRAFT)
  await page.getByRole("button", { name: "Log out" }).click()
  await expect(page).toHaveURL(/\/login$/)
  await page.getByLabel("Email").fill("e2e@genesy.test")
  await page.getByLabel("Password").fill("e2e-password-only")
  await page.getByRole("button", { name: "Log in" }).click()
  await expect(page).toHaveURL(/\/$/)
  await expect(page.getByLabel(v.message)).toHaveValue("")
})

test.describe("approving after the login expired", () => {
  test.use({
    allow: {
      responses: [
        { url: "/api/voice/confirm", status: 401 },
        { url: "/api/voice/session", status: 401 },
      ],
    },
  })

  test("P8-4 the refused approval is reachable again after logging in, and nothing ran", async ({ page }) => {
    const agent = await fakeAgent(page)
    const confirms = await stockRoutes(page)
    // Office still holds the review: the refused confirm never reached it.
    await page.route("**/api/voice/tools", async (route) => {
      const body = route.request().postDataJSON() as Record<string, unknown>
      if (body.command === "status")
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
      if (body.command === "restore")
        return route.fulfill({ json: stockReview(120, "stock-120-restored") })
      return route.fallback()
    })
    const { expire } = await server(page, ["POST /api/voice/confirm", "DELETE /api/voice/session"])
    await proposeStock(page, agent, 120)
    expire()
    await scene(page).getByRole("button", { name: v.approve }).click()
    await logInAgain(page)
    await expect(page).toHaveURL(/\/$/)
    const prompt = page.getByRole("region", { name: k.resume.title })
    await expect(prompt).toContainText("Set stock levels per warehouse")
    await prompt.getByRole("button", { name: k.resume.continue }).click()
    await expect(scene(page).getByRole("button", { name: v.approve })).toBeVisible()
    // The refused click never reached Office: no decision was recorded.
    expect(confirms).toEqual([])
  })
})
