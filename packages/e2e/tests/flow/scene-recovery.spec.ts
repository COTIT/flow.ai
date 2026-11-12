// INT-3: a scene read answers 503, the line then drops for a moment (a tunnel), then comes back.
// The scene must never stay a skeleton: it shows data, or a calm notice, within a bounded time.
import { markInjected } from "../../src/test"
import { expect, test } from "./storefront-api"

test.beforeEach(({ api }) => void api)

const READS = /\/(sales-summary|attention|orders)(\/|\?|$)/

test("a 503, then a short offline blip, then back: no skeleton is left behind", async ({
  page,
  context,
}) => {
  let broken = true
  markInjected(page, READS)
  await page.route(READS, (route) =>
    broken
      ? route.fulfill({ status: 503, json: { message: "down" } })
      : route.fallback()
  )
  await page.goto("/scenes/pulse")
  // The first answers are 503 and their retries are waiting; now the line drops.
  await page.waitForTimeout(500)
  await context.setOffline(true)
  await page.waitForTimeout(3_000)
  await expect(page.locator("[data-loading]")).toHaveCount(0)
  await expect(page.locator("main")).toContainText("You're offline")
  broken = false
  await context.setOffline(false)
  await expect(page.locator('[data-component="sales-today"]')).toContainText(
    /\d/,
    { timeout: 15_000 }
  )
  await expect(page.locator("[data-loading]")).toHaveCount(0)
})
