import { seedSession } from "../../src/session"
import { expect, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { fakeAgent, v } from "./voice"

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

test.use({
  permissions: ["microphone"],
  launchOptions: {
    args: [
      "--use-fake-device-for-media-stream",
      "--use-fake-ui-for-media-stream",
    ],
  },
})

test("the microphone opens only after Start, and can be muted", async ({
  page,
}) => {
  const agent = await fakeAgent(page)
  await page.goto("/")
  expect(await page.evaluate(() => Reflect.get(window, "micRequests"))).toBe(0)
  await page.getByRole("button", { name: v.start, exact: true }).click()
  await expect(page.getByRole("button", { name: v.end })).toBeVisible()
  await expect
    .poll(() => page.evaluate(() => Reflect.get(window, "micRequests")))
    .toBeGreaterThan(0)
  await expect.poll(() => agent.connections).toBe(1)
  const status = page.getByRole("status").filter({
    hasText: new RegExp(
      [v.activity.listening, v.activity.holdToTalk, v.activity.muted].join("|")
    ),
  })
  await expect(status.first()).toBeVisible()
})
