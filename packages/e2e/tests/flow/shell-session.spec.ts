import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { chat, fakeAgent, send, startText, v, watch } from "./voice"

// SIMULATED voice: mock upstream behind Flow's real proxy + a fake ElevenLabs socket.

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi())
  await page.addInitScript(() => {
    Reflect.set(window, "micRequests", 0)
    const real = navigator.mediaDevices?.getUserMedia?.bind(
      navigator.mediaDevices
    )
    if (real)
      navigator.mediaDevices.getUserMedia = (constraints) => {
        Reflect.set(
          window,
          "micRequests",
          Number(Reflect.get(window, "micRequests")) + 1
        )
        return real(constraints)
      }
  })
})

test("opening Flow starts nothing: no session, no microphone, no socket", async ({
  page,
}) => {
  const sessions = watch(page, "session")
  const agent = await fakeAgent(page)
  await page.goto("/")
  await expect(
    page.getByRole("button", { name: v.start, exact: true })
  ).toBeVisible()
  await expect(
    page.getByRole("button", { name: v.textInstead, exact: true })
  ).toBeVisible()
  await expect(page.getByText(v.welcome)).toBeVisible()
  await expect(page.getByText(v.micHint)).toBeVisible()
  // Give a stray auto-start time to show itself.
  await page.waitForTimeout(500)
  expect(sessions).toEqual([])
  expect(agent.connections).toBe(0)
  expect(await page.evaluate(() => Reflect.get(window, "micRequests"))).toBe(0)
  await expectNoHorizontalScroll(page)
})

test("a text conversation starts on click, carries messages both ways and ends", async ({
  page,
}) => {
  const sessions = watch(page, "session")
  const agent = await fakeAgent(page, (text, a) => a.say(`You said: ${text}`))
  await page.goto("/")
  await startText(page)
  expect(sessions.filter((s) => s.method === "POST")).toHaveLength(1)
  expect(agent.connections).toBe(1)
  await send(page, "Hello from Flow")
  const log = page.getByRole("log", { name: v.transcript })
  await expect(log).toContainText("Hello from Flow")
  await expect(log).toContainText("You said: Hello from Flow")
  expect(await page.evaluate(() => Reflect.get(window, "micRequests"))).toBe(0)
  await expectNoHorizontalScroll(page)

  await page.getByRole("button", { name: v.end }).click()
  await expect(
    page.getByRole("button", { name: v.textInstead, exact: true })
  ).toBeVisible()
  await expect
    .poll(() => sessions.some((s) => s.method === "DELETE"))
    .toBe(true)
  // Ending clears the conversation: nothing of it stays on screen.
  await expect(page.getByText("Hello from Flow")).toHaveCount(0)
})

test("the header links to the storefront review", async ({ page }) => {
  await page.goto("/")
  const link = page.getByRole("link", { name: chat.storefront })
  await expect(link).toHaveAttribute("href", "/storefront")
})
