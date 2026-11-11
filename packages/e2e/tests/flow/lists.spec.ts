// Lists board (canvas 8AQVf7XEiV1cSyqPi5G6p9, Lists.dc.html) over a SIMULATED API: list scenes
// are a table at full width, the same columns with a ⋯ row menu when narrower (an open
// conversation rail counts), and labelled cards on a phone.
import type { Locator, Page } from "@playwright/test"

import { expectNoHorizontalScroll, expectSingleScroller } from "../../src/test"
import { rowAction } from "./lists"
import { HONEY, TEA, sc } from "./scenes-api"
import { s } from "./shell"
import { expect, test } from "./storefront-api"
import { fakeAgent, send, startText } from "./voice"

test.beforeEach(({ api }) => void api)

type Form = "table" | "menu" | "cards"
const formOf = async (page: Page): Promise<Form> => {
  const w = await page.locator("[data-attention]").evaluate((el) => el.clientWidth)
  return w >= 980 ? "table" : w >= 600 ? "menu" : "cards"
}
const stock = (page: Page) => page.locator('[data-group="stock.below_minimum"]')
const item = (page: Page, id: string) => stock(page).locator(`[data-item="${id}"]`)

async function expectForm(page: Page, row: Locator, form: Form) {
  const more = row.getByRole("button", { name: /^Actions for / })
  const ack = row.getByRole("button", { name: sc.noticed.acknowledge, exact: true })
  if (form === "menu") {
    await expect(more).toBeVisible()
    await expect(ack).toBeHidden()
  } else {
    await expect(more).toBeHidden()
    await expect(ack).toBeVisible()
    await expect(row.getByRole("button", { name: /^Snooze / })).toBeVisible()
  }
  if (form === "cards") {
    // Labelled numbers; the whole name, never cut off.
    await expect(row.getByText(sc.available, { exact: true })).toBeVisible()
    await expect(row.getByText(sc.shortfall, { exact: true })).toBeVisible()
    const cut = await row
      .getByRole("rowheader")
      .evaluate((b) => b.scrollWidth > b.clientWidth + 1)
    expect(cut).toBe(false)
  } else {
    for (const name of [sc.table.product, sc.available, sc.minimum, sc.shortfall])
      await expect(
        stock(page).getByRole("columnheader", { name, exact: true })
      ).toBeVisible()
  }
  // Numbers right-aligned in a table, the shortfall bold; every control is a 44 px target.
  const cells = row.getByRole("cell")
  if (form !== "cards")
    await expect(cells.nth(0)).toHaveCSS("text-align", "right")
  expect(Number(await cells.nth(2).evaluate((c) => getComputedStyle(c).fontWeight))).toBeGreaterThanOrEqual(600)
  for (const b of await row.getByRole("button").all())
    if (await b.isVisible())
      expect((await b.boundingBox())!.height).toBeGreaterThanOrEqual(44)
}

test("Flow noticed: table, row menu or labelled cards by width; acknowledge and snooze", async ({
  page,
  api,
}) => {
  await page.goto("/scenes/noticed")
  const honey = item(page, HONEY)
  await expect(honey).toContainText("Raw Honey 500g")
  const form = await formOf(page)
  const width = page.viewportSize()!.width
  expect(form).toBe(width >= 1024 ? "table" : width >= 640 ? "menu" : "cards")
  await expectForm(page, honey, form)
  await expectSingleScroller(page)
  await expectNoHorizontalScroll(page)

  await rowAction(page, honey, "acknowledge")
  await expect(honey.locator("[data-acknowledged]:visible")).toHaveText(
    sc.noticed.acknowledged
  )
  await rowAction(page, item(page, TEA), "tomorrow")
  await expect(item(page, TEA)).toHaveCount(0)
  expect(api.callsTo("PUT", "/workspace/w1/attention/marks").map((c) => c.body)).toEqual([
    { kind: "stock.below_minimum", ref_id: HONEY, state: "acknowledged" },
    { kind: "stock.below_minimum", ref_id: TEA, state: "snoozed", snooze: "until_tomorrow" },
  ])
  await expectSingleScroller(page)
})

test("row actions work from the keyboard", async ({ page, api }) => {
  await page.goto("/scenes/noticed")
  const tea = item(page, TEA)
  await expect(tea).toBeVisible()
  await expect(tea.getByRole("button").first()).toBeVisible()
  const more = tea.getByRole("button", { name: /^Actions for / })
  const trigger = (await more.isVisible())
    ? more
    : tea.getByRole("button", { name: /^Snooze / })
  await trigger.focus()
  await page.keyboard.press("Enter")
  await expect(page.getByRole("menu")).toBeVisible()
  // Keyboard focus moves into the menu (an item, or the menu itself first).
  await expect
    .poll(() => page.evaluate(() => !!document.activeElement?.closest("[role=menu]")))
    .toBe(true)
  if (!(await page.evaluate(() => document.activeElement?.getAttribute("role") === "menuitem")))
    await page.keyboard.press("ArrowDown")
  await page.keyboard.press("Enter")
  await expect
    .poll(() => api.callsTo("PUT", "/workspace/w1/attention/marks").length)
    .toBe(1)
  const body = api.callsTo("PUT", "/workspace/w1/attention/marks")[0]!.body
  expect(body).toMatchObject({ ref_id: TEA })
})

test("with the conversation rail open, a 1280 screen shows the row-menu form", async ({
  page,
}) => {
  test.skip(page.viewportSize()!.width < 1024, "desktop with the rail")
  await fakeAgent(page, (text, a) => a.say(`Answer: ${text}`))
  await page.goto("/")
  await startText(page)
  await send(page, "Hello")
  await expect(page.getByRole("log")).toContainText("Answer: Hello")
  await page.getByRole("button", { name: s.backHome }).click()
  await page.getByRole("link", { name: sc.noticed.review }).click()
  await expect(page).toHaveURL(/\/scenes\/noticed$/)
  await expect(page.getByRole("complementary", { name: s.conversation })).toBeVisible()
  const honey = item(page, HONEY)
  await expect(honey).toBeVisible()
  expect(await formOf(page)).toBe("menu")
  await expectForm(page, honey, "menu")
  await expectSingleScroller(page)
  await expectNoHorizontalScroll(page)
})

test("stock, orders and invoices follow the same rule", async ({ page }) => {
  await page.goto("/scenes/stock")
  const row = page.locator(`[data-row="${HONEY}"]`)
  await expect(row).toBeVisible()
  await expect(row.getByRole("cell").nth(2)).toContainText("6")
  if (page.viewportSize()!.width < 640)
    await expect(row.getByText(sc.shortfall, { exact: true })).toBeVisible()
  else
    await expect(
      page.getByRole("columnheader", { name: sc.shortfall, exact: true })
    ).toBeVisible()
  await expectSingleScroller(page)
  await expectNoHorizontalScroll(page)
})
