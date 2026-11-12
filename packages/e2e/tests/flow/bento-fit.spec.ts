// Bento fit (plan child-bento-fit-plan.md): each scene fits the screen at a glance on iPad and desktop.
// Targets: grid empty ≤ 5%, no tile > 15% empty height, the first tiles fully visible, nothing cut
// without a count, reading order = saved order. SIMULATED API. Red until the build lands (BF-1).
import type { Page } from "@playwright/test"

import { expectSingleScroller } from "../../src/test"
import { DEFAULTS, sceneWorld } from "./scenes-api"
import { expect, test } from "./storefront-api"
import { fakeAgent, v } from "./voice"

const SIZES = [
  [1180, 820],
  [1366, 1024],
  [820, 1180],
  [1280, 800],
  [1920, 1080],
] as const
const SCENES = [
  "pulse",
  "stock",
  "orders",
  "sales",
  "noticed",
  "routines",
] as const
/** Red on main until the fit build lands (BF-1); Flow noticed and My Routines already fit. */
const BF1 =
  "BF-1 open: tiles sized by type (empty grid, empty tile height, cut lists); see child-bento-fit-plan.md"
const NOT_YET_FIT = new Set(["pulse", "stock", "orders", "sales"])
/** The first tiles a person must see whole, without scrolling. */
const PRIMARY = 4

/** What the scene looks like at a glance. */
async function glance(page: Page) {
  return page.evaluate(() => {
    const view = document
      .querySelector("[data-flow-scroll]")!
      .getBoundingClientRect()
    const tiles = [
      ...document.querySelectorAll<HTMLElement>("[data-component]"),
    ]
    const grid = tiles[0]?.parentElement?.getBoundingClientRect()
    let covered = 0
    const each = tiles.map((tile) => {
      const r = tile.getBoundingClientRect()
      covered += r.width * r.height
      // The content's own height: the lowest visible descendant, never the stretched card itself.
      let bottom = r.top
      for (const el of tile.querySelectorAll<HTMLElement>("*")) {
        if (el.closest("[data-tile-footer]")) continue
        const b = el.getBoundingClientRect()
        if (b.height > 0 && b.height < r.height - 4)
          bottom = Math.max(bottom, b.bottom)
      }
      const footer = tile.querySelector("[data-tile-footer]")
      return {
        id: tile.dataset.component!,
        emptyPct: Math.round(
          (Math.max(0, r.height - Math.min(r.height, bottom - r.top + 16)) /
            r.height) *
            100
        ),
        whole: r.top >= view.top - 1 && r.bottom <= view.bottom + 1,
        cutWithoutCount: !!footer && !/\(\d+\)/.test(footer.textContent ?? ""),
      }
    })
    return {
      emptyPct: grid
        ? Math.round((1 - covered / (grid.width * grid.height)) * 100)
        : 0,
      each,
    }
  })
}

async function expectFits(page: Page, scene: string) {
  await expect(page.locator("[data-component]").first()).toBeVisible()
  await page.waitForTimeout(1_200) // data and tile growth settle
  const g = await glance(page)
  expect(
    g.each.map((t) => t.id),
    "reading order = saved order"
  ).toEqual((DEFAULTS[scene] ?? []).map((c) => c.id))
  expect(g.emptyPct, "empty grid area %").toBeLessThanOrEqual(5)
  for (const t of g.each) {
    expect(t.emptyPct, `${t.id}: empty height %`).toBeLessThanOrEqual(15)
    expect(t.cutWithoutCount, `${t.id}: cut without a count`).toBe(false)
  }
  for (const t of g.each.slice(0, PRIMARY))
    expect(t.whole, `${t.id}: whole on the first screen`).toBe(true)
}

for (const [width, height] of SIZES)
  for (const scene of SCENES)
    test(`${scene} fits at a glance at ${width}×${height}`, async ({
      page,
      api,
    }) => {
      test.fail(NOT_YET_FIT.has(scene), BF1)
      sceneWorld(api)
      await page.setViewportSize({ width, height })
      await page.goto(`/scenes/${scene}`)
      await expectFits(page, scene)
    })

for (const [width, height] of [
  [1180, 820],
  [1920, 1080],
] as const)
  for (const scene of ["pulse", "stock", "sales"] as const)
    test(`${scene} fits beside the open conversation at ${width}×${height}`, async ({
      page,
      api,
    }) => {
      test.fail(true, BF1)
      sceneWorld(api)
      await fakeAgent(page, (text, a) => a.say(`Answer: ${text}`))
      await page.setViewportSize({ width, height })
      await page.goto(`/scenes/${scene}`)
      await page.getByLabel(v.message).fill("Sales today?")
      await page.getByLabel(v.message).press("Enter")
      await expect(page.locator('[data-flow-pane="rail"]')).toBeVisible()
      await expectFits(page, scene)
    })

test("50 products below minimum: Stock Watch shows the rows that fit and how many there are", async ({
  page,
  api,
}) => {
  test.fail(true, BF1)
  sceneWorld(api, { stockRows: 50 })
  await page.setViewportSize({ width: 1180, height: 820 })
  await page.goto("/scenes/stock")
  await expectFits(page, "stock")
})

test("no sales and nothing low: Morning Pulse shows slim lines, never big empty boxes", async ({
  page,
  api,
}) => {
  test.fail(true, BF1)
  sceneWorld(api, { noSales: true, stockRows: 0 })
  await page.setViewportSize({ width: 1180, height: 820 })
  await page.goto("/scenes/pulse")
  await expectFits(page, "pulse")
})

test("a phone stacks the scene in one scroller", async ({ page, api }) => {
  sceneWorld(api)
  await page.setViewportSize({ width: 375, height: 812 })
  await page.goto("/scenes/pulse")
  await expect(page.locator("[data-component]").first()).toBeVisible()
  await expectSingleScroller(page)
})
