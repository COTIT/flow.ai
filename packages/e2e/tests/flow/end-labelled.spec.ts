// P6: End says its name on every screen size (not only an icon on phones). SIMULATED voice.
import type { Locator } from "@playwright/test"

import { expect, test } from "./storefront-api"
import { fakeAgent, v, typeInstead } from "./voice"

test.beforeEach(({ api }) => void api)

/** The word a person sees on the button: drawn text, not the screen reader's clipped copy. */
const seenWords = (button: Locator) =>
  button.evaluate((el) =>
    [...el.querySelectorAll("span")]
      .filter((s) => {
        const box = s.getBoundingClientRect()
        return box.width > 2 && box.height > 2 && getComputedStyle(s).display !== "none"
      })
      .map((s) => s.textContent?.trim())
      .join(" ")
  )

test("the conversation's End shows the word End and ends the call", async ({ page }) => {
  await fakeAgent(page, (_, a) => setTimeout(() => a.say("Hello."), 50))
  await page.goto("/")
  await page.getByRole("button", { name: typeInstead, exact: true }).click()
  await page.getByLabel(v.message).fill("Hello")
  await page.getByRole("button", { name: v.send, exact: true }).click()
  const end = page.locator("[data-conversation-header]").getByRole("button", { name: v.end })
  await expect(end).toBeVisible()
  await expect.poll(() => seenWords(end)).toMatch(/^End/)
  const box = (await end.boundingBox())!
  expect(box.height).toBeGreaterThanOrEqual(44)
  await end.click()
  await expect(page.getByText("Call ended.").first()).toBeVisible()
})

test("on a scene, End beside the conversation also says End (rail, sheet or bar)", async ({ page }) => {
  await fakeAgent(page, (_, a) => setTimeout(() => a.say("Hello."), 50))
  await page.goto("/scenes/sales")
  await page.getByLabel(v.message).fill("Hello")
  await page.getByRole("button", { name: v.send, exact: true }).click()
  const ends = page.getByRole("button", { name: v.end })
  await expect(ends.first()).toBeVisible()
  // One End in sight, and it says its name.
  const shown = await ends.evaluateAll((els) => els.filter((e) => (e as HTMLElement).offsetParent !== null).length)
  expect(shown).toBe(1)
  const end = ends.filter({ visible: true }).first()
  await expect.poll(() => seenWords(end)).toMatch(/^End/)
  expect((await end.boundingBox())!.height).toBeGreaterThanOrEqual(44)
})
