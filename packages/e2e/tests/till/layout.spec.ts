import type { Locator, Page } from "@playwright/test"

import { expectTillLook } from "./look"
import { expect, seedTill, test } from "./fake-till"

/** Landscape at tablet width or more puts panes side by side; portrait and phones stack them. */
function sideBySide(page: Page) {
  const { width, height } = page.viewportSize()!
  return width > height && width > 760
}

async function box(l: Locator) {
  const b = await l.boundingBox()
  expect(b, "element has a box").not.toBeNull()
  return b!
}

/** `b` sits right of `a` (side by side) or below it (stacked), as the viewport asks. */
async function expectArranged(page: Page, a: Locator, b: Locator) {
  const [first, second] = [await box(a), await box(b)]
  if (sideBySide(page)) {
    expect(second.x, "beside").toBeGreaterThanOrEqual(first.x + first.width)
  } else {
    expect(second.y, "below").toBeGreaterThanOrEqual(first.y + first.height)
  }
}

test("the sale screen puts the basket beside the tiles in landscape and below them in portrait", async ({
  page,
}) => {
  await seedTill(page)
  await page.goto("/sale")
  const tiles = page.getByRole("list", { name: "Products" })
  const basket = page.getByRole("complementary", { name: "Basket" })
  await expect(tiles.getByRole("button")).toHaveCount(1)
  await expectArranged(page, tiles, basket)
  await tiles.getByRole("button").click()
  // Charge is reachable without scrolling the page, on every screen.
  await expect(basket.getByRole("button", { name: /^Charge/ })).toBeInViewport()
  await expectTillLook(page)
})

test("the PIN screen puts the users beside the pad in landscape and above it in portrait", async ({
  page,
}) => {
  await seedTill(page, { loggedIn: false })
  await page.goto("/login")
  const users = page.getByRole("list", { name: "Till users" })
  await expect(users.getByRole("button")).toHaveCount(2)
  await expectArranged(
    page,
    page.locator("section", { has: users }),
    page.getByRole("form", { name: "Enter your PIN" })
  )
  // Layout shift on this screen is login.spec's check (and fails at 375 px on main).
  await expectTillLook(page, { shift: false })
})
