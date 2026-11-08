import { cachedUrls, expectInstallable, waitForServiceWorker } from "../../src/pwa"
import { expect, expectNoHorizontalScroll, expectTapTargets, test } from "../../src/test"

test("can be added to the home screen as 'Office'", async ({ page }) => {
  await page.goto("/login")
  await expectInstallable(page, { name: "Genesy Office", shortName: "Office", startUrl: "/" })
})

test("with no connection, a page shows a plain 'you are offline' screen, not a browser error", async ({
  page,
  context,
}) => {
  await page.goto("/login")
  await waitForServiceWorker(page)
  await context.setOffline(true)
  await page.goto("/orders")
  await expect(page.getByText("You are offline")).toBeVisible()
  await expect(page.getByText("Check your connection, then try again.")).toBeVisible()
  await expect(page.getByRole("button", { name: "Try again" })).toBeVisible()
  await expectNoHorizontalScroll(page)
  await expectTapTargets(page)

  // Back online, "Try again" reaches the real page.
  await context.setOffline(false)
  await page.getByRole("button", { name: "Try again" }).click()
  await expect(page.getByText("You are offline")).toBeHidden()
})

test("keeps no API answer on the device", async ({ page }) => {
  await page.goto("/login")
  await waitForServiceWorker(page)
  const urls = await cachedUrls(page)
  expect(urls.length).toBeGreaterThan(0)
  expect(urls.filter((u) => new URL(u).pathname.startsWith("/v1"))).toEqual([])
})
