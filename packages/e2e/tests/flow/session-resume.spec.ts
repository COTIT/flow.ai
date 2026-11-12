import type { Page } from "@playwright/test"

import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { sc } from "./scenes-api"
import { revealHeader } from "./shell"
import { ACTION, k, proposeStock, readBack, scene, stockReview, stockRoutes } from "./task"
import { fakeAgent, startText, v, typeInstead } from "./voice"

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
    // Orb-first home has no message box: the draft is typed in a scene's dock.
    await page.getByRole("region", { name: sc.yourFlow }).getByRole("link").first().click()
    await expect(page).toHaveURL(/\/scenes\//)
    await page.getByLabel(v.message).fill(DRAFT)
    expire()
    await page.getByLabel(v.message).press("Enter")
    await logInAgain(page)
    // Logging in again goes back to the scene the person was on (P8-3), the draft with it.
    await expect(page).toHaveURL(/\/scenes\//)
    await expect(page.getByLabel(v.message)).toHaveValue(DRAFT)
  })

  test("P8-1 a draft is not given to another person who logs in on the same tab", async ({ page }) => {
    await fakeAgent(page)
    const { expire } = await server(page, ["POST /api/voice/session"])
    await page.goto("/")
    // Orb-first home has no message box: the draft is typed in a scene's dock.
    await page.getByRole("region", { name: sc.yourFlow }).getByRole("link").first().click()
    await expect(page).toHaveURL(/\/scenes\//)
    await page.getByLabel(v.message).fill(DRAFT)
    expire()
    await page.getByLabel(v.message).press("Enter")
    // Someone else logs in: /me now names another user.
    await page.route("**/v1/me", (route) =>
      route.fulfill({ json: { data: { user: { id: "u-other", first_name: "Other", permissions: [] } } } })
    )
    await logInAgain(page)
    await expect(page.getByLabel(v.message)).toHaveValue("")
  })
})

test("P8-1 a draft is dropped when the person logs out by choice", async ({ page }) => {
  await page.goto("/")
  // Orb-first home has no message box: the draft is typed in a scene's dock.
  await page.getByRole("region", { name: sc.yourFlow }).getByRole("link").first().click()
  await expect(page).toHaveURL(/\/scenes\//)
  await page.getByLabel(v.message).fill(DRAFT)
  await (await revealHeader(page)).getByRole("button", { name: "Log out" }).click()
  // While a conversation is live, Log out asks first (P10-3).
  const ask = page.getByRole("alertdialog")
  if (await ask.waitFor({ state: "visible", timeout: 1_000 }).then(() => true, () => false))
    await ask.getByRole("button", { name: "Log out" }).click()
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

const batchSteps = (done: number, total = 21) =>
  Array.from({ length: total }, (_, i) => ({
    status: i < done ? "completed" : "not_started",
    data: { actionId: `a-${i}`, operation: "products_update", summary: `Price ${i}` },
  }))
/** Office's batch status (batch-controller.ts): progress only, never a verified completion. */
const batchTask = (state: string, done: number) => ({
  status: state === "running" ? "running" : "partial",
  data: {
    kind: "batch",
    batchId: "b-1",
    state,
    nextIndex: done,
    summary: "Update 21 supplier purchase prices",
    recoverable: false,
    steps: batchSteps(done),
  },
})

test.describe("a run going when the login expired", () => {
  test.use({
    allow: {
      responses: [
        { url: "/api/voice/batch", status: 401 },
        { url: "/api/voice/confirm", status: 401 },
        { url: "/api/voice/session", status: 401 },
      ],
    },
  })

  test("P8-6 its state is reachable after logging in again, and nothing resumes", async ({ page }) => {
    const agent = await fakeAgent(page)
    const commands: string[] = []
    const confirms: string[] = []
    let atOffice = batchTask("running", 4)
    let expired = () => {}
    const loggedOut = new Promise<void>((resolve) => (expired = resolve))
    await page.route("**/api/voice/batch", async (route) => {
      const body = route.request().postDataJSON() as Record<string, unknown>
      commands.push(String(body.command))
      if (body.command === "prepare")
        return route.fulfill({
          json: {
            status: "requires_confirmation",
            data: { batchId: "b-1" },
            review: {
              ticket: "batch-21",
              kind: "batch",
              batchId: "b-1",
              summary: "Update 21 supplier purchase prices",
              confirmationCode: "246810",
              confirmationPhrase: "Confirm batch 246810",
              details: { notIncluded: [] },
              steps: batchSteps(0).map((s) => ({
                actionId: s.data.actionId,
                operationId: "products_update",
                summary: s.data.summary,
                details: {},
              })),
              expiresAt: new Date(Date.now() + 10 * 60_000).toISOString(),
            },
          },
        })
      if (body.command === "status") return route.fulfill({ json: atOffice })
      return route.fallback()
    })
    await page.route("**/api/voice/confirm", async (route) => {
      confirms.push(String((route.request().postDataJSON() as Record<string, unknown>).decision))
      await loggedOut // the run is still going at Office when the login expires
      return route.fulfill({ status: 401, json: { message: "Unauthenticated." } })
    })
    await page.route("**/api/voice/tools", async (route) => {
      const body = route.request().postDataJSON() as Record<string, unknown>
      if (body.command === "status") return route.fulfill({ json: { status: "completed", data: [atOffice] } })
      return route.fallback()
    })
    const { expire } = await server(page, ["POST /api/voice/batch", "DELETE /api/voice/session"])

    await page.goto("/")
    await startText(page)
    agent.call("assistant_batch_prepare", { calls: [{}, {}] })
    await expect(page.locator("[data-batch]")).toContainText("21 changes matched")
    await scene(page).getByRole("button", { name: v.approve }).click()
    await expect.poll(() => confirms).toEqual(["approve"])

    // Office pauses the run when the conversation goes (8 done, 13 not run); the login expired.
    atOffice = batchTask("paused", 8)
    expire()
    expired()
    await logInAgain(page)
    await expect(page).toHaveURL(/\/$/)
    const prompt = page.getByRole("region", { name: k.resume.outcomes })
    await expect(prompt).toContainText("Update 21 supplier purchase prices")
    await expect(prompt).toContainText("Paused")
    await expect(prompt).toContainText("8 updated · 0 failed · 13 not run · 0 need checking")
    await expect(prompt.getByRole("button", { name: k.receipt.check })).toBeVisible()
    // Shown, never resumed: no new decision and no run command after logging in.
    expect(confirms).toEqual(["approve"])
    expect(commands.filter((c) => c !== "status" && c !== "prepare")).toEqual([])
    await expectNoHorizontalScroll(page)
    // Asking to check is a question in a new conversation, never a replay of the run.
    await prompt.getByRole("button", { name: k.receipt.check }).click()
    await expect
      .poll(() => agent.sent.filter((e) => e.type === "user_message").map((e) => e.text))
      .toContain(k.receipt.checkMessage)
    expect(confirms).toEqual(["approve"])
    expect(commands.filter((c) => c !== "status" && c !== "prepare")).toEqual([])
  })
})

test("a run shown while going is named again once it ends, then never again", async ({ page }) => {
  await fakeAgent(page)
  let atOffice: object = batchTask("running", 4)
  await page.route("**/api/voice/tools", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>
    if (body.command === "status") return route.fulfill({ json: { status: "completed", data: [atOffice] } })
    return route.fallback()
  })
  await page.goto("/")
  await startText(page) // a conversation in this tab: its recovery pointer is kept
  const prompt = page.getByRole("region", { name: k.resume.outcomes })

  // Each visit is a reload: the conversation is gone, the run at Office is not.
  await page.reload()
  await expect(prompt).toContainText("Running.")
  await expect(prompt).toContainText("4 updated · 0 failed · 17 not run")
  await page.reload()
  await expect(prompt).toContainText("Running.") // still going: not marked seen

  atOffice = {
    status: "completed",
    data: { ...batchTask("completed", 21).data, state: "completed" },
  }
  await page.reload()
  await expect(prompt).toContainText("Update 21 supplier purchase prices · it ran.")
  await expect(prompt).toContainText("21 updated · 0 failed · 0 not run · 0 need checking")
  await page.reload()
  await expect(page.getByRole("button", { name: typeInstead, exact: true })).toBeVisible()
  await expect(prompt).toHaveCount(0)
})
