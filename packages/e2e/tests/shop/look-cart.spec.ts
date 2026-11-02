import type { Locator } from "@playwright/test"

import {
  expect,
  expectNoHorizontalScroll,
  expectNoLayoutShift,
  expectTapTargets,
  test,
} from "../../src/test"
import { seedCart } from "./cart"

// Mocks `ShopCart` and `ShopStates` ("Empty cart") in docs/design/web-redesign/canvas.
const NAME = "Ilundo Wild Forest Honey"

const css = (l: Locator, prop: string) =>
  l.evaluate((el, p) => getComputedStyle(el).getPropertyValue(p), prop)

test("the cart is a list tile beside a summary tile", async ({
  page,
  isMobile,
}) => {
  await seedCart(page, 2)
  await page.goto("/cart")

  const h1 = page.getByRole("heading", { level: 1, name: "Your cart" })
  expect(await css(h1, "font-size")).toBe("22px")
  await expect(
    page.getByRole("link", { name: "Keep shopping" })
  ).toHaveAttribute("href", "/")

  const lines = page.getByRole("list", { name: "Your cart" })
  const line = lines.getByRole("listitem")
  await expect(line).toHaveCount(1)
  await expect(line).toContainText(`${NAME} · Size: 500 g`)
  await expect(line).toContainText("$24.99 each")
  await expect(line.getByLabel("Quantity")).toHaveText("2")
  await expect(
    line.getByRole("button", { name: `Remove ${NAME}` })
  ).toBeVisible()

  const summary = page.getByRole("region", { name: "Summary" })
  await expect(summary).toContainText("Items")
  await expect(summary).toContainText("2")
  await expect(summary).toContainText(/Total calculated at checkout/)
  const checkout = summary.getByRole("link", { name: "Go to checkout" })
  expect(await css(checkout, "height")).toBe("52px")

  // Side by side on a desktop, stacked on a phone.
  const listBox = (await lines.boundingBox())!
  const summaryBox = (await summary.boundingBox())!
  if (isMobile) expect(summaryBox.y).toBeGreaterThan(listBox.y + listBox.height)
  else expect(summaryBox.x).toBeGreaterThan(listBox.x + listBox.width)

  await expectNoHorizontalScroll(page)
  if (isMobile) await expectTapTargets(page)
  // The cart lives in the browser: a skeleton in its shape holds the place until it loads.
  await expectNoLayoutShift(page)

  await line.getByRole("button", { name: `One more ${NAME}` }).click()
  await expect(line.getByLabel("Quantity")).toHaveText("3")
  await expect(summary).toContainText("3")
  await line.getByRole("button", { name: `Remove ${NAME}` }).click()
  await expect(page.getByText("Your cart is empty")).toBeVisible()
})

test("an empty cart is the empty state inside a card", async ({ page }) => {
  await page.goto("/cart")
  const empty = page.locator('[data-slot="empty-state"]')
  await expect(empty).toContainText("Your cart is empty")
  await expect(
    empty.getByRole("link", { name: "Browse the shop" })
  ).toBeVisible()
  await expectNoHorizontalScroll(page)
})
