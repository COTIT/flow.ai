// Orb-first home (SIMULATED voice): the orb is the one primary action; no message box on home, so
// nothing is ever sent out of sight. Scenes show the conversation again on each send or reply.
import type { Page } from "@playwright/test"

import { s } from "./shell"
import { expect, test } from "./storefront-api"
import { readBack, stockRoutes } from "./task"
import { fakeAgent, v } from "./voice"

test.beforeEach(({ api }) => void api)

const answering = (page: Page) =>
  fakeAgent(page, (text, a) => setTimeout(() => a.say(`Found: ${text}`), 120))
const composer = (page: Page) => page.locator("[data-flow-composer]")
const rail = (page: Page) => page.locator("#flow-rail")
const ends = (page: Page) => page.getByRole("button", { name: v.end })

/** "Type instead" from home, one message and its reply, then back to home. */
async function talkThenHome(page: Page, text = "Which orders wait?") {
  await page.goto("/")
  await page.getByRole("button", { name: s.typeInstead }).click()
  await expect(composer(page), "the field is ready to type").toBeFocused()
  await composer(page).fill(text)
  await page.getByRole("button", { name: v.send, exact: true }).click()
  await expect(page.getByText(`Found: ${text}`).first()).toBeVisible()
  await page.waitForTimeout(1600) // the turn settles
  await page.getByRole("button", { name: s.backHome }).first().click()
}

async function noOverflow(page: Page) {
  const below = await page.evaluate(
    () => document.documentElement.scrollHeight - window.innerHeight
  )
  expect(below, "page scrolls below the screen").toBeLessThanOrEqual(0)
}

/** Every size: the projects' own (1280, 834, 375) plus a wide screen and phone landscape. */
const sizes = [
  { width: 1920, height: 1080 },
  { width: 640, height: 400 },
]

function homeTests() {
  test("idle home: the orb and its words say Tap to talk; Type instead starts typing", async ({
    page,
  }) => {
    await answering(page)
    await page.goto("/")
    await expect(
      page.getByRole("button", { name: s.tapToTalk })
    ).not.toHaveCount(0)
    await expect(composer(page), "no message box on home").toBeHidden()
    // Keyboard: the orb first, then Type instead.
    await page.locator("#flow-content").focus()
    await page.keyboard.press("Tab")
    await expect(page.locator(":focus")).toHaveAccessibleName(s.tapToTalk)
    await page.keyboard.press("Tab")
    await expect(page.locator(":focus")).toHaveAccessibleName(s.typeInstead)
    await noOverflow(page)
  })

  test("a running conversation on home: Continue, its last line and End; no hidden sends", async ({
    page,
  }) => {
    await answering(page)
    await talkThenHome(page)
    await expect(rail(page), "no rail on home").toHaveCount(0)
    await expect(composer(page), "no message box on home").toBeHidden()
    const label = page.getByRole("button", { name: s.continueConversation })
    await expect(label).not.toHaveCount(0)
    await expect(
      page.getByText("Found: Which orders wait?").first()
    ).toBeVisible()
    await expect(ends(page)).toHaveCount(1)
    await expect(ends(page)).toContainText("End")
    await noOverflow(page)
    // The words open the conversation where it left off, like the orb.
    await page.getByText(s.continueConversation, { exact: true }).click()
    await expect(composer(page)).toBeVisible()
    await expect(page.getByRole("log").first()).toContainText(
      "Found: Which orders wait?"
    )
  })

  test("a hide remembered from home or a review never hides the conversation on home", async ({
    page,
  }) => {
    await page.addInitScript(() =>
      sessionStorage.setItem(
        "flow.rail",
        JSON.stringify({
          open: { "desktop:review": false, "tablet:review": false },
        })
      )
    )
    await answering(page)
    await talkThenHome(page)
    await expect(
      page.getByRole("button", { name: s.continueConversation })
    ).not.toHaveCount(0)
    await expect(ends(page)).toHaveCount(1)
  })

  test("a suggestion with a question opens the conversation and its answer shows", async ({
    page,
  }) => {
    // A past use ranks the "Weekly sales" question among home's suggestions.
    await page.addInitScript(() =>
      localStorage.setItem(
        "genesy.flow.activity:u1:w1",
        JSON.stringify({
          v: 1,
          uses: { weekly_sales: { n: 9, last: Date.now() } },
          hintDismissed: false,
        })
      )
    )
    await answering(page)
    await page.goto("/")
    await page
      .getByRole("region", { name: s.dock })
      .getByRole("button", { name: "Weekly sales" })
      .click()
    await expect(page.getByRole("log").first()).toContainText(
      "How were sales this week?"
    )
    await expect(page.getByRole("log").first()).toContainText(
      "Found: How were sales this week?"
    )
  })
}

homeTests()
for (const viewport of sizes)
  test.describe(`at ${viewport.width}x${viewport.height}`, () => {
    test.use({ viewport })
    test.beforeEach(({ page }, info) => {
      void page
      test.skip(info.project.name !== "flow-mobile", "one extra run per size")
    })
    homeTests()
  })

test.describe("scenes", () => {
  test("a send or a reply shows a hidden conversation again; Hide hands focus to Show", async ({
    page,
  }) => {
    const agent = await answering(page)
    await talkThenHome(page)
    await page
      .getByRole("region", { name: /Your Flow/ })
      .getByRole("link", { name: /Order Focus/ })
      .click()
    await expect(page).toHaveURL(/\/scenes\/orders$/)
    const hide = page.getByRole("button", { name: s.hideConversation }).first()
    const lower = page.getByRole("button", { name: s.lowerConversation })
    if (await hide.isVisible()) {
      await hide.click()
      await expect(
        page.getByRole("button", { name: s.showConversation }),
        "focus goes on at the way back"
      ).toBeFocused()
    } else if (await lower.isVisible()) await lower.click()
    await composer(page).fill("And the honey stock?")
    await page.getByRole("button", { name: v.send, exact: true }).click()
    await expect(
      page.getByText("Found: And the honey stock?").first(),
      "the send shows the conversation"
    ).toBeVisible()
    await expect(
      page.getByRole("button", { name: v.send, exact: true }),
      "focus stays where it was"
    ).toBeFocused()

    // Hidden again; a reply arriving on its own shows it, focus untouched.
    await page.waitForTimeout(1600)
    if (
      await page
        .getByRole("button", { name: s.hideConversation })
        .first()
        .isVisible()
    )
      await page
        .getByRole("button", { name: s.hideConversation })
        .first()
        .click()
    else await page.getByRole("button", { name: s.lowerConversation }).click()
    await page.waitForTimeout(300) // lowering hands focus to the handle a frame later
    await composer(page).focus()
    agent.say("Raw Honey 500g is 6 below its minimum.")
    await expect(
      page.getByText("Raw Honey 500g is 6 below its minimum.").first()
    ).toBeVisible()
    await expect(composer(page)).toBeFocused()
    await noOverflow(page)
  })

  test("a review arriving on a scene shows the conversation, even after a hide in a review", async ({
    page,
    api,
  }, info) => {
    test.skip(
      info.project.name === "flow-mobile",
      "the rail: tablet and desktop"
    )
    await page.addInitScript(() =>
      sessionStorage.setItem(
        "flow.rail",
        JSON.stringify({
          open: { "desktop:review": false, "tablet:review": false },
        })
      )
    )
    const stock =
      readBack("120.00")["GET /workspace/w1/products/p-500/stock-with-bins"]!
    api.on(
      "GET",
      "/workspace/:ws/products/:id/stock-with-bins",
      () => stock.body as object
    )
    await stockRoutes(page)
    const agent = await answering(page)
    await talkThenHome(page)
    await page
      .getByRole("region", { name: /Your Flow/ })
      .getByRole("link", { name: /Stock Watch/ })
      .click()
    await expect(page).toHaveURL(/\/scenes\/stock$/)
    const hide = page.getByRole("button", { name: s.hideConversation }).first()
    if (await hide.isVisible()) await hide.click()
    await expect(rail(page)).toHaveCount(0)
    agent.call("stock_set_levels", {
      path: { product: "p-500" },
      body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 120 }] },
    })
    await expect(rail(page), "the review shows the conversation").toHaveCount(1)
  })
})
