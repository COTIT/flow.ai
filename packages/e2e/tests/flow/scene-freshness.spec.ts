// The "Updated …" line is coarse: it says "just now" for the first minute, then whole minutes.
// Routines refetch only every 5 min, so the data stays the same age while the fake clock runs.
import { expect, test } from "./storefront-api"

test.beforeEach(({ api }) => void api)

// Viewports (desktop, tablet, 375px mobile) come from the Playwright projects.
test("freshness line is quiet, then whole minutes", async ({ page }) => {
  const problems: string[] = []
  page.on("pageerror", (e) => problems.push(e.message))
  page.on("console", (m) => {
    if (m.type() === "error") problems.push(m.text())
  })
  await page.clock.install()
  await page.goto("/scenes/routines")
  const line = page.locator("[data-freshness]")
  await expect(line).toHaveAttribute("data-freshness", "fresh")
  await expect(line).toContainText("Updated just now")
  // Texts never tick per second: same words at 1 s and at 20 s.
  await page.clock.fastForward(1_000)
  await expect(line).toContainText("Updated just now")
  await page.clock.fastForward(19_000)
  await expect(line).toContainText("Updated just now")
  // The screen reader line stays silent for the quiet text.
  await expect(line.locator("[aria-live]")).toHaveText("")
  await page.clock.fastForward(41_000)
  await expect(line).toContainText("Updated 1 min ago")
  // Past twice the scene refresh time (2 min) the line warns instead of counting on.
  await page.clock.fastForward(60_000)
  await expect(line).toHaveAttribute("data-freshness", "stale")
  expect(problems).toEqual([])
})
