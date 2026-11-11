import { expect, test, designs } from "./storefront-api"
import { fakeAgent, startText, v, watch } from "./voice"
import { dock, phase, s } from "./shell"

// SIMULATED voice: mock upstream behind Flow's real proxy + a fake ElevenLabs socket.

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    Reflect.set(window, "micRequests", 0)
    const real = navigator.mediaDevices?.getUserMedia?.bind(
      navigator.mediaDevices
    )
    if (real)
      navigator.mediaDevices.getUserMedia = (constraints) => {
        Reflect.set(window, "micRequests", Number(Reflect.get(window, "micRequests")) + 1)
        return real(constraints)
      }
  })
})

test("unsent text stays in the dock across scenes", async ({ page, api }) => {
  designs(api)
  await page.goto("/")
  const box = dock(page).getByLabel(v.message)
  await box.fill("half a thought")
  await page.getByRole("link", { name: "Storefront" }).click()
  await expect(page).toHaveURL(/\/storefront$/)
  await expect(box).toHaveValue("half a thought")
  await page.getByRole("button", { name: s.backHome }).click()
  await expect(box).toHaveValue("half a thought")
})

test("typing without a conversation starts a text one (never the microphone) and sends", async ({
  page,
  api,
}) => {
  designs(api)
  const sessions = watch(page, "session")
  const agent = await fakeAgent(page, (text, a) => a.say(`You said: ${text}`))
  await page.goto("/")
  const box = dock(page).getByLabel(v.message)
  await box.fill("How are sales today?")
  await box.press("Enter")
  await expect(page.getByRole("log")).toContainText("You said: How are sales today?")
  await expect(box).toHaveValue("")
  await expect(phase(page)).toHaveText(new RegExp(s.phase.live))
  expect(sessions.filter((r) => r.method === "POST")).toHaveLength(1)
  expect(agent.connections).toBe(1)
  expect(await page.evaluate(() => Reflect.get(window, "micRequests"))).toBe(0)
})

test("typed “Hey Genesy, …” starts and sends only the instruction", async ({
  page,
  api,
}) => {
  designs(api)
  const agent = await fakeAgent(page)
  await page.goto("/")
  await dock(page).getByLabel(v.message).fill("Hey Genesy, show my pending orders")
  await dock(page).getByRole("button", { name: v.send, exact: true }).click()
  await expect
    .poll(() => agent.sent.filter((e) => e.type === "user_message").map((e) => e.text))
    .toEqual(["show my pending orders"])
})

test("text works inside a scene, in the same conversation", async ({ page, api }) => {
  designs(api)
  const agent = await fakeAgent(page, (text, a) => a.say(`Done: ${text}`))
  await page.goto("/")
  await startText(page)
  await page.getByRole("link", { name: "Storefront" }).click()
  await dock(page).getByLabel(v.message).fill("Make the hero calmer")
  await dock(page).getByLabel(v.message).press("Enter")
  await expect
    .poll(() => agent.sent.filter((e) => e.type === "user_message").map((e) => e.text))
    .toEqual(["Make the hero calmer"])
  expect(agent.connections).toBe(1)
})

test("text sent while the conversation is still opening is kept and goes out once connected", async ({
  page,
  api,
}) => {
  designs(api)
  // Hold the session start so the conversation is visibly still opening.
  await page.route("**/api/voice/session", async (route) => {
    if (route.request().method() === "POST")
      await new Promise((done) => setTimeout(done, 800))
    await route.continue()
  })
  const agent = await fakeAgent(page)
  await page.goto("/")
  await page.getByRole("button", { name: v.textInstead, exact: true }).click()
  const box = dock(page).getByLabel(v.message)
  await box.fill("Sent while opening")
  await box.press("Enter")
  await expect
    .poll(() => agent.sent.filter((e) => e.type === "user_message").map((e) => e.text))
    .toEqual(["Sent while opening"])
  await expect(box).toHaveValue("")
})

test.describe("a start that fails", () => {
  test.use({
    allow: { responses: [{ url: "/api/voice/session", status: 503 }] },
  })

  test("forgets the text it was going to send; a later conversation never sends it", async ({
    page,
    api,
  }) => {
    designs(api)
    let refuse = true
    await page.route("**/api/voice/session", async (route) => {
      if (route.request().method() === "POST" && refuse) {
        refuse = false
        return route.fulfill({
          status: 503,
          contentType: "application/json",
          body: JSON.stringify({ error: "unavailable" }),
        })
      }
      await route.continue()
    })
    const agent = await fakeAgent(page)
    await page.goto("/")
    const box = dock(page).getByLabel(v.message)
    await box.fill("Old question")
    await box.press("Enter")
    // The start fails and says so.
    await expect(dock(page).getByRole("alert")).toBeVisible()
    await expect(phase(page)).not.toHaveText(new RegExp(s.phase.live))
    // The text stays in the box for the person to decide.
    await expect(box).toHaveValue("Old question")
    await page.getByRole("button", { name: v.textInstead, exact: true }).click()
    await expect(dock(page).getByRole("button", { name: v.addFiles })).toBeVisible()
    await page.waitForTimeout(700)
    expect(agent.sent.filter((e) => e.type === "user_message")).toEqual([])
  })
})

test("Enter before the readiness answer still starts the text conversation and sends", async ({
  page,
  api,
}) => {
  designs(api)
  await page.route("**/api/voice/readiness", async (route) => {
    await new Promise((done) => setTimeout(done, 1000))
    await route.continue()
  })
  const agent = await fakeAgent(page)
  await page.goto("/")
  const box = dock(page).getByLabel(v.message)
  await box.fill("Early question")
  await box.press("Enter")
  await expect
    .poll(() => agent.sent.filter((e) => e.type === "user_message").map((e) => e.text))
    .toEqual(["Early question"])
  await expect(box).toHaveValue("")
})
