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
