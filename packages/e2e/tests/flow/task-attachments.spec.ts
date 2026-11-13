import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { fakeAgent, startText, v, watch } from "./voice"

// SIMULATED voice: mock upstream behind Flow's real proxy + a fake ElevenLabs socket.

// A 1x1 PNG.
const PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
  "base64"
)
const photo = { name: "shelf.png", mimeType: "image/png", buffer: PNG }

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi())
})

test("a reference file uploads through Flow with the message, and ending clears it", async ({
  page,
}) => {
  const uploads = watch(page, "attachments")
  const agent = await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  await page.getByLabel(v.chooseFiles).setInputFiles(photo)
  const tray = page.getByRole("list", { name: v.attachments })
  await expect(tray).toContainText("1. shelf.png")
  await expect(tray).toContainText(v.fileStates.selected)
  await expectNoHorizontalScroll(page)

  await page.getByLabel(v.message).fill("What is on this shelf?")
  await page.getByRole("button", { name: v.send, exact: true }).click()
  await expect(tray).toContainText(v.fileStates.submitted)
  expect(uploads.filter((u) => u.method === "POST")).toHaveLength(1)
  // The agent gets the provider's file id, never the file itself.
  await expect
    .poll(() => JSON.stringify(agent.sent))
    .toContain("provider-file-1")

  await page.getByRole("button", { name: v.endCall }).click()
  await expect(tray).toBeHidden()
  await expect(page.getByText("shelf.png")).toHaveCount(0)
})

test("a file chosen by mistake is removed before anything is sent", async ({
  page,
}) => {
  const uploads = watch(page, "attachments")
  await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  await page.getByLabel(v.chooseFiles).setInputFiles(photo)
  await page
    .getByRole("button", { name: /shelf\.png/ })
    .last()
    .click()
  await expect(page.getByRole("list", { name: v.attachments })).toBeHidden()
  expect(uploads).toEqual([])
})

test.describe("an upload the service refuses", () => {
  test.use({
    allow: { responses: [{ url: "/api/voice/attachments", status: 415 }] },
  })

  test("says why and can be retried", async ({ page }) => {
    let refuse = true
    await page.route("**/api/voice/attachments", (route) =>
      refuse
        ? route.fulfill({ status: 415, json: { message: "Unsupported." } })
        : route.fallback()
    )
    await fakeAgent(page)
    await page.goto("/")
    await startText(page)
    await page.getByLabel(v.chooseFiles).setInputFiles(photo)
    await page.getByLabel(v.message).fill("Read this")
    await page.getByRole("button", { name: v.send, exact: true }).click()
    const tray = page.getByRole("list", { name: v.attachments })
    await expect(tray.getByRole("alert")).toContainText(v.fileErrors.type)
    await expect(tray).toContainText(v.fileStates.failed)
    refuse = false
    await tray
      .getByRole("button", { name: /1/ })
      .filter({ hasText: v.retry })
      .click()
    await expect(tray).toContainText(v.fileStates.submitted)
  })
})
