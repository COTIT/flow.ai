import type { Locator } from "@playwright/test"

import {
  expect,
  expectNoHorizontalScroll,
  expectNoLayoutShift,
  expectTapTargets,
  test,
} from "../../src/test"
import { seedCart } from "./cart"

// Mock `ShopCheckout` in docs/design/web-redesign/canvas.
const css = (l: Locator, prop: string) =>
  l.evaluate((el, p) => getComputedStyle(el).getPropertyValue(p), prop)

test("checkout is two field cards beside the order card", async ({
  page,
  isMobile,
}) => {
  await seedCart(page, 2)
  await page.goto("/checkout")

  const h1 = page.getByRole("heading", { level: 1, name: "Checkout" })
  expect(await css(h1, "font-size")).toBe("22px")
  const steps = page.getByRole("list", { name: "Checkout steps" })
  await expect(steps.getByRole("listitem")).toHaveCount(3)
  await expect(steps.locator('[aria-current="step"]')).toContainText("2")

  const contact = page.getByRole("region", { name: "Contact" })
  await expect(contact.getByLabel("Email")).toBeVisible()
  await expect(contact.getByLabel("Email")).toHaveAccessibleDescription(
    "Your receipt and order link go here."
  )
  await expect(contact.getByLabel("Phone (optional)")).toBeVisible()
  await expect(contact.getByLabel("Full name")).toBeVisible()
  const address = page.getByRole("region", { name: "Shipping address" })
  for (const field of ["Address", "Postal code", "City", "Country"])
    await expect(address.getByLabel(field, { exact: true })).toBeVisible()

  const order = page.getByRole("region", { name: "Your order" })
  await expect(order).toContainText("Ilundo Wild Forest Honey · Size: 500 g")
  await expect(order).toContainText("2 × $24.99")
  const place = order.getByRole("button", { name: "Place order" })
  expect(await css(place, "height")).toBe("52px")

  const contactBox = (await contact.boundingBox())!
  const orderBox = (await order.boundingBox())!
  if (isMobile) expect(orderBox.y).toBeGreaterThan(contactBox.y)
  else expect(orderBox.x).toBeGreaterThan(contactBox.x + contactBox.width)

  await expectNoHorizontalScroll(page)
  if (isMobile) await expectTapTargets(page)
  await expectNoLayoutShift(page)
})

test("missing fields: one error notice on top, each field says what to fix", async ({
  page,
}) => {
  await seedCart(page, 1)
  await page.goto("/checkout")
  await page.getByRole("button", { name: "Place order" }).click()
  const error = page
    .getByRole("alert")
    .filter({ hasText: "Please check the highlighted fields" })
  await expect(error).toHaveAttribute("data-variant", "error")
  // The notice leads the form column, above the first card.
  const errorBox = (await error.boundingBox())!
  const contactBox = (await page
    .getByRole("region", { name: "Contact" })
    .boundingBox())!
  expect(errorBox.y).toBeLessThan(contactBox.y)
  await expect(page.getByLabel("Postal code")).toHaveAccessibleDescription(
    "Please fill this in."
  )
  await expectNoHorizontalScroll(page)
})
