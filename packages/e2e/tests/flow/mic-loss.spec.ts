import type { Page } from "@playwright/test"

import { seedSession } from "../../src/session"
import { expect, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { fakeAgent, send, v, tapOrb } from "./voice"

// SIMULATED voice + Chromium's fake microphone. P7-1 / P7-2 (voice-first persona).

test.use({
  permissions: ["microphone"],
  launchOptions: {
    args: [
      "--use-fake-device-for-media-stream",
      "--use-fake-ui-for-media-stream",
    ],
  },
})

/** The microphone under the test's control: refuse the next request, or take it away. */
async function micControl(page: Page) {
  await page.addInitScript(() => {
    const mic = { deny: false, tracks: [] as MediaStreamTrack[] }
    Reflect.set(window, "__mic", mic)
    const media = navigator.mediaDevices
    const real = media?.getUserMedia?.bind(media)
    if (!real) return
    media.getUserMedia = async (constraints) => {
      if (mic.deny)
        throw new DOMException("Permission denied", "NotAllowedError")
      const stream = await real(constraints)
      mic.tracks.push(...stream.getAudioTracks())
      return stream
    }
  })
  type Mic = { deny: boolean; tracks: MediaStreamTrack[] }
  return {
    deny: () =>
      page.evaluate(
        () => void ((Reflect.get(window, "__mic") as Mic).deny = true)
      ),
    /** Unplugged or revoked by the OS: every live track ends. */
    revoke: () =>
      page.evaluate(() => {
        for (const track of (Reflect.get(window, "__mic") as Mic).tracks) {
          track.stop()
          track.dispatchEvent(new Event("ended"))
        }
      }),
  }
}

async function startVoice(page: Page) {
  await tapOrb(page) // orb-first home: the orb starts voice
  await expect(page.getByRole("button", { name: v.endCall })).toBeVisible()
  // The attach button exists only once the session is verified.
  await expect(page.getByRole("button", { name: v.addFiles })).toBeVisible()
}

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi())
})

test("P7-1: a refused microphone at Start says so in Flow's words", async ({
  page,
}) => {
  const mic = await micControl(page)
  await fakeAgent(page)
  await page.goto("/")
  await mic.deny()
  await tapOrb(page) // orb-first home: the orb starts voice
  const alert = page.getByRole("alert").first()
  await expect(alert).toContainText(v.microphoneDenied)
  await expect(alert).not.toContainText("Permission denied")
})

test("P7-2: the microphone taken away mid-call pauses voice; typing still works", async ({
  page,
}) => {
  const mic = await micControl(page)
  const agent = await fakeAgent(page, (text, a) => a.say(`You said: ${text}`))
  await page.goto("/")
  await startVoice(page)
  await mic.revoke()
  await expect(page.getByText(v.capturePaused).first()).toBeVisible()
  await expect(page.getByRole("button", { name: v.resumeVoice })).toBeVisible()
  // Text stays available while the microphone is gone.
  await send(page, "Sales today?")
  await expect
    .poll(() => agent.sent.filter((e) => e.type === "user_message").length)
    .toBe(1)
})
