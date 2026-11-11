import type { Page } from "@playwright/test"

import { seedSession } from "../../src/session"
import { expect, expectTapTargets, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { fakeAgent, startText, v } from "./voice"

// SIMULATED voice: mock upstream behind Flow's real proxy + a fake ElevenLabs socket.
// P4-2: ten reference files (the most allowed) must never push the dock past the screen.

// A 1x1 PNG.
const PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
  "base64"
)
const files = (count: number) =>
  Array.from({ length: count }, (_, n) => ({
    name: `shelf-${n + 1}.png`,
    mimeType: "image/png",
    buffer: PNG,
  }))

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi())
})

/** Attaches files, then checks the page stays inside the screen with the composer in view. */
async function filesFit(page: Page, count: number) {
  await fakeAgent(page)
  await page.goto("/")
  await startText(page)
  await page.getByLabel(v.chooseFiles).setInputFiles(files(count))
  const tray = page.getByRole("list", { name: v.attachments })
  await expect(tray.getByRole("listitem")).toHaveCount(count)

  // One scroll root: the page never grows below or beside the screen, and no element besides
  // the conversation (`main`) scrolls vertically (a sideways tray is fine).
  const found = await page.evaluate(() => {
    const d = document.documentElement
    const others = Array.from(document.querySelectorAll<HTMLElement>("body *"))
      .filter((el) => {
        // The scroll root and the side-by-side panes (the dock's own pane included) may scroll.
        if (el.matches("main, [data-flow-pane]")) return false
        const y = getComputedStyle(el).overflowY
        return /(auto|scroll)/.test(y) && el.scrollHeight > el.clientHeight + 1
      })
      .map(
        (el) =>
          `${el.tagName.toLowerCase()}.${String(el.className).slice(0, 60)}`
      )
    return {
      below: d.scrollHeight - window.innerHeight,
      sideways: d.scrollWidth - window.innerWidth,
      others,
    }
  })
  expect(found.below, "page scrolls below the screen").toBeLessThanOrEqual(0)
  expect(found.sideways, "page scrolls sideways").toBeLessThanOrEqual(0)
  expect(found.others, "a second vertical scroll bar").toEqual([])

  await expect(page.getByLabel(v.message)).toBeInViewport()
  await expect(
    page.getByRole("button", { name: v.send, exact: true })
  ).toBeInViewport()
  await expectTapTargets(page, { within: `ul[aria-label="${v.attachments}"]` })

  // The last file is still reachable and removable, and its name stays whole in the title.
  const last = `shelf-${count}.png`
  await expect(tray.getByTitle(last)).toHaveCount(1)
  await tray.getByRole("button", { name: last }).click()
  await expect(tray.getByRole("listitem")).toHaveCount(count - 1)
}

test("ten reference files keep the page inside the screen", async ({
  page,
}) => {
  await filesFit(page, 10)
})

// Short and small phones, where the persona saw the page overflow from the first file on:
// checked once, from the 375 px project.
for (const [width, height, counts] of [
  [812, 375, [1, 3, 10]],
  [320, 568, [3, 10]],
  [375, 667, [4, 10]],
] as const) {
  test.describe(`at ${width}x${height}`, () => {
    test.use({ viewport: { width, height } })
    for (const count of counts)
      test(`${count} reference files keep the page inside the screen`, async ({
        page,
      }, info) => {
        test.skip(
          info.project.name !== "flow-mobile",
          "one phone run is enough"
        )
        await filesFit(page, count)
      })
  })
}
