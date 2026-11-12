// One-tap End call where the conversation lives: the rail header, the phone sheet's line, the
// scene bar while the rail is collapsed, and the dock row on short screens (SIMULATED voice).
import type { Page } from "@playwright/test"

import { s } from "./shell"
import { expect, test } from "./storefront-api"
import { fakeAgent, startText, v, watch } from "./voice"

test.beforeEach(({ api }) => void api)

const ends = (page: Page) => page.getByRole("button", { name: v.end })
const live = (page: Page) => page.getByRole("button", { name: v.addFiles })
const dock = (page: Page) => page.getByRole("region", { name: s.dock })

/** A text call, then the conversation view closed: the dashboard with the rail or the sheet. */
async function callOnDashboard(page: Page) {
  await fakeAgent(page)
  await page.goto("/")
  await expect(ends(page)).toHaveCount(0)
  await startText(page)
  await expect(ends(page), "the conversation view's own End").toHaveCount(1)
  await page.getByRole("button", { name: s.backHome }).first().click()
  await expect(ends(page)).toHaveCount(1)
}

/** Orb-first home shows End beside "Continue your conversation"; the rail or sheet live on scenes. */
async function onScene(page: Page) {
  await expect(ends(page), "home's End").toContainText(s.endCall)
  await page
    .getByRole("region", { name: /Your Flow/ })
    .getByRole("link", { name: /Morning Pulse/ })
    .click()
  await expect(page).toHaveURL(/\/scenes\/pulse$/)
  await expect(ends(page)).toHaveCount(1)
}

async function noOverflow(page: Page) {
  const below = await page.evaluate(
    () => document.documentElement.scrollHeight - window.innerHeight
  )
  expect(below, "page scrolls below the screen").toBeLessThanOrEqual(0)
}

test("End shows once in every view, only while connected, and one tap ends the call", async ({
  page,
}) => {
  const sessions = watch(page, "session")
  await callOnDashboard(page)
  await noOverflow(page)
  await onScene(page)
  const raise = page.getByRole("button", { name: s.raiseConversation })
  if (await raise.isVisible()) {
    // Phone: the sheet's line, lowered and raised.
    await raise.click()
    await expect(
      page.getByRole("button", { name: s.lowerConversation })
    ).toBeVisible()
    await expect(ends(page)).toHaveCount(1)
    await noOverflow(page)
  } else {
    // Rail: its header; collapsed, beside Show conversation.
    const rail = page.getByRole("complementary", { name: s.conversation })
    if (await rail.isVisible())
      await expect(rail.getByRole("button", { name: v.end })).toBeVisible()
    const hide = page.getByRole("button", { name: s.hideConversation }).first()
    if (await hide.isVisible()) await hide.click() // a tablet opens scenes with it collapsed
    await expect(
      page.getByRole("button", { name: s.showConversation })
    ).toBeVisible()
    await expect(ends(page)).toHaveCount(1)
    await noOverflow(page)
  }
  await ends(page).click()
  await expect(live(page), "the call ended").toBeHidden()
  await expect
    .poll(() => sessions.filter((r) => r.method === "DELETE").length)
    .toBeGreaterThan(0)
  await expect(ends(page)).toHaveCount(0)
})

test("after End from the keyboard, focus goes on at the orb or the dock, never to the page", async ({
  page,
}) => {
  await callOnDashboard(page)
  await ends(page).focus()
  await page.keyboard.press("Enter")
  await expect(live(page)).toBeHidden()
  const focused = page.locator(":focus")
  await expect(focused).toHaveCount(1)
  // Home: the orb ("Tap to talk"); a scene's dock: its mic or the message box.
  expect(await focused.getAttribute("aria-label")).toMatch(
    new RegExp(`^(${s.tapToTalk}|${v.start}|${v.message})$`)
  )
})

// The smallest phone, and phone landscape where the sheet's line is too slim for a 44 px
// button: there End sits in the dock row, Send keeps its place. Checked from the 375 px project.
for (const [width, height] of [
  [320, 568],
  [640, 400],
] as const)
  test.describe(`at ${width}x${height}`, () => {
    test.use({ viewport: { width, height } })
    test("End shows once, ends the call, and Send keeps its place", async ({
      page,
    }, info) => {
      test.skip(info.project.name !== "flow-mobile", "one phone run is enough")
      await page.goto("/")
      await callOnDashboard(page)
      await noOverflow(page)
      await onScene(page)
      const end = ends(page)
      if (height <= 500) {
        await expect(
          dock(page).getByRole("button", { name: v.end }),
          "in the dock row on a short screen"
        ).toBeVisible()
        // Send keeps the row's end; End sits before the mic, the field keeps its room.
        const x = async (name: string) =>
          (await dock(page)
            .getByRole("button", { name, exact: true })
            .boundingBox())!
        const [endBox, sendBox] = [await x(v.end), await x(v.send)]
        const row = (await dock(page).locator("form").boundingBox())!
        expect(Math.round(sendBox.x + sendBox.width), "Send ends the row").toBe(
          Math.round(row.x + row.width)
        )
        expect(endBox.x, "End before Send").toBeLessThan(
          sendBox.x - sendBox.width
        )
        const field = await page.getByLabel(v.message).boundingBox()
        expect(field!.width, "the field keeps its room").toBeGreaterThanOrEqual(
          100
        )
      }
      const box = await end.boundingBox()
      expect(box!.height, "a 44 px target").toBeGreaterThanOrEqual(44)
      await page.getByRole("button", { name: s.raiseConversation }).click()
      await expect(ends(page)).toHaveCount(1)
      await noOverflow(page)
      await end.click()
      await expect(live(page)).toBeHidden()
      await expect(ends(page)).toHaveCount(0)
    })
  })
