import { expectNoHorizontalScroll } from "../../src/test"
import { expect, seedTill, test, WS } from "./fake-till"

test.describe("unknown screen", () => {
  test.use({ allow: { responses: [{ url: "/no-such-screen", status: 404 }] } })

  test("says so and links back to the sale screen", async ({ page }) => {
    await page.goto("/no-such-screen")
    await expect(
      page.getByRole("status").filter({ hasText: "There is no such screen." })
    ).toBeVisible()
    await expect(
      page.getByRole("link", { name: "Go to the sale screen" })
    ).toBeVisible()
    await expectNoHorizontalScroll(page)
  })
})

test("going offline shows a warning; coming back clears it", async ({
  page,
  context,
}) => {
  await seedTill(page)
  await page.goto("/sale")
  await expect(page.getByRole("heading", { name: "Sale" })).toBeVisible()
  await context.setOffline(true)
  const offline = page
    .getByRole("status")
    .filter({ hasText: "Offline. Sales are kept on this device" })
  await expect(offline).toBeVisible()
  await expectNoHorizontalScroll(page)
  await context.setOffline(false)
  await expect(offline).toHaveCount(0)
})

test.describe("expired session", () => {
  test.use({
    allow: {
      responses: [{ url: `/v1/workspace/${WS}/till/me`, status: 401 }],
    },
  })

  test("returns to the PIN screen and says why", async ({ page, api }) => {
    api.on("GET", "/workspace/:ws/till/me", () => ({
      status: 401,
      body: { message: "Unauthenticated." },
    }))
    await seedTill(page)
    await page.goto("/sale")
    await expect(page).toHaveURL("/login")
    await expect(
      page
        .getByRole("status")
        .filter({ hasText: "Your session ended. Log in again to continue." })
    ).toBeVisible()
  })
})

test("a user's own logout shows no session-ended message", async ({ page }) => {
  await seedTill(page)
  await page.goto("/sale")
  await page.getByRole("button", { name: "Log out" }).click()
  await expect(page).toHaveURL("/login")
  await expect(page.getByText("Your session ended.")).toHaveCount(0)
})
