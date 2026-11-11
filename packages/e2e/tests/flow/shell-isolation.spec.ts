import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { flowApi, ILUNDO, mockApi, RUAHA } from "./api"
import { fakeAgent, send, startText, v, watch } from "./voice"

// SIMULATED voice: mock upstream behind Flow's real proxy + a fake ElevenLabs socket.

test("switching workspace ends the conversation and shows nothing of the old one", async ({
  page,
}) => {
  await seedSession(page, { workspaceIds: ["w1", "w2"] })
  await mockApi(
    page,
    flowApi({ "GET /me/workspaces": { body: { data: [ILUNDO, RUAHA] } } })
  )
  const sessions = watch(page, "session")
  await fakeAgent(page, (text, a) => a.say(`Ilundo answer to ${text}`))
  await page.goto("/")
  await startText(page)
  await send(page, "Ilundo stock")
  const log = page.getByRole("log", { name: v.transcript })
  await expect(log).toContainText("Ilundo answer to Ilundo stock")

  const picker = page.getByRole("combobox", { name: "Workspace" })
  await picker.click()
  await page
    .getByRole("option", { name: RUAHA.display_name, exact: true })
    .click()
  await expect(
    page.getByRole("button", { name: v.textInstead, exact: true })
  ).toBeVisible()
  await expect
    .poll(() => sessions.some((s) => s.method === "DELETE"))
    .toBe(true)
  expect(sessions.find((s) => s.method === "DELETE")!.body).toMatchObject({
    workspaceId: "w1",
  })
  await expect(page.getByText("Ilundo answer to Ilundo stock")).toHaveCount(0)
  await expect(page.getByText("Ilundo stock")).toHaveCount(0)
  await expectNoHorizontalScroll(page)
})

test("logging out ends the voice session first, then shows no conversation", async ({
  page,
}) => {
  await seedSession(page)
  await mockApi(page, flowApi())
  const order: string[] = []
  page.on("request", (r) => {
    const path = new URL(r.url()).pathname
    if (path === "/api/voice/session" && r.method() === "DELETE")
      order.push(`voice:${r.headers().authorization}`)
    if (path === "/v1/logout") order.push("logout")
  })
  await fakeAgent(page, (text, a) => a.say(`Private answer to ${text}`))
  await page.goto("/")
  await startText(page)
  await send(page, "private question")
  await expect(page.getByRole("log")).toContainText("Private answer")
  await page.getByRole("button", { name: "Log out" }).click()
  await expect(page).toHaveURL("/login")
  expect(order[0]).toBe("voice:Bearer e2e-token")
  expect(order).toContain("logout")
  await expect(page.getByText("Private answer")).toHaveCount(0)
})

test("a voice end that hangs never blocks logging out", async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi())
  await page.route("**/api/voice/session", (route) =>
    route.request().method() === "DELETE"
      ? undefined // never answered
      : route.fallback()
  )
  await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  const started = Date.now()
  await page.getByRole("button", { name: "Log out" }).click()
  await expect(page).toHaveURL("/login", { timeout: 10_000 })
  expect(Date.now() - started).toBeLessThan(8_000)
})

test.describe("the voice service says the login has expired", () => {
  test.use({
    allow: { responses: [{ url: "/api/voice/session", status: 401 }] },
  })

  test("signs out and the login page says the session ended", async ({
    page,
  }) => {
    await seedSession(page)
    await mockApi(page, flowApi())
    await page.route("**/api/voice/session", (route) =>
      route.fulfill({ status: 401, json: { message: "Unauthenticated." } })
    )
    await page.goto("/")
    await page.getByRole("button", { name: v.textInstead, exact: true }).click()
    await expect(page).toHaveURL("/login")
    await expect(
      page
        .getByRole("status")
        .filter({ hasText: "Your session ended. Log in again to carry on." })
    ).toBeVisible()
  })
})

test.describe("the voice service refuses", () => {
  test.use({
    allow: { responses: [{ url: "/api/voice/session", status: 403 }] },
  })

  test("says why in plain words, with no raw server text", async ({ page }) => {
    await seedSession(page)
    await mockApi(page, flowApi())
    await page.route("**/api/voice/session", (route) =>
      route.fulfill({
        status: 403,
        json: { message: "Raw backend detail that must stay off screen" },
      })
    )
    await page.goto("/")
    await page.getByRole("button", { name: v.textInstead, exact: true }).click()
    await expect(
      page.getByRole("alert").filter({ hasText: v.requestErrors.forbidden })
    ).toBeVisible()
    await expect(
      page.getByText("Raw backend detail that must stay off screen")
    ).toHaveCount(0)
    await expectNoHorizontalScroll(page)
  })
})

test.describe("the connection drops mid-answer", () => {
  test("offers to continue in a new conversation and starts one only on click", async ({
    page,
  }) => {
    await seedSession(page)
    await mockApi(page, flowApi())
    const sessions = watch(page, "session")
    const agent = await fakeAgent(page, (_text, a) => a.close())
    await page.goto("/")
    await startText(page)
    await send(page, "How much did we sell?")
    const resume = page.getByRole("button", { name: v.unfinished.continue })
    await expect(resume).toBeVisible()
    // Starts only: a dropped call is also reported on this route ({ command: "dropped" }).
    const starts = () =>
      sessions.filter(
        (s) => s.method === "POST" && !(s.body as { command?: string }).command
      ).length
    expect(starts()).toBe(1)
    await resume.click()
    await expect.poll(starts).toBe(2)
    await expect.poll(() => agent.connections).toBe(2)
    await expect(page.getByRole("log")).toContainText(v.unfinished.continued)
  })
})
