import type { Page } from "@playwright/test"

import {
  NO_PROVIDER_EMAIL,
  PRODUCT,
  STOCK_LIMIT,
} from "../../mock/fixtures.mjs"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"

/** Puts `quantity` of the honey's 500 g jar in the cart from its product page. */
async function addHoney(page: Page, quantity: number) {
  await page.goto(`/products/${PRODUCT.honey}`)
  await page.getByLabel("Size or option").selectOption({ label: "Size: 500 g" })
  await page.getByLabel("Quantity").fill(String(quantity))
  await page.getByRole("button", { name: "Add to cart" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "Added to your cart." })
  ).toBeVisible()
}

async function fillCustomer(page: Page, email = "buyer@e2e.test") {
  await page.getByLabel("Email").fill(email)
  await page.getByLabel("Full name").fill("Ada Buyer")
  await page.getByLabel("Address", { exact: true }).fill("1 Main St")
  await page.getByLabel("Postal code").fill("10001")
  await page.getByLabel("City").fill("New York")
  await page.getByLabel("Country").fill("US")
}

test("buy honey: cart, checkout, the server's totals, pay, thank you", async ({
  page,
}) => {
  await addHoney(page, 2)
  await expect(page.getByRole("link", { name: "Cart, 2 items" })).toBeVisible()

  await page.getByRole("link", { name: "View cart" }).click()
  await expect(
    page.getByRole("heading", { level: 1, name: "Your cart" })
  ).toBeVisible()
  await expect(page.getByText("Size: 500 g")).toBeVisible()
  await expect(page.getByText("$24.99 each")).toBeVisible()
  // No total in the cart: the order the server creates is the authority.
  await expect(page.getByText(/Total calculated at checkout/)).toBeVisible()
  await page
    .getByRole("button", { name: "One more Ilundo Wild Forest Honey" })
    .click()
  await expect(page.getByRole("link", { name: "Cart, 3 items" })).toBeVisible()
  await expectNoHorizontalScroll(page)

  await page.getByRole("link", { name: "Go to checkout" }).click()
  await fillCustomer(page)
  await expectNoHorizontalScroll(page)
  await page.getByRole("button", { name: "Place order" }).click()

  await expect(
    page.getByRole("heading", { level: 1, name: /^Order WEB-\d+$/ })
  ).toBeVisible()
  await expect(page.getByText("Waiting for payment.")).toBeVisible()
  await expect(page.locator("dd").last()).toHaveText("$74.97")
  await expectNoHorizontalScroll(page)

  await page.getByRole("button", { name: "Pay now" }).click()
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Thank you! Your payment is in" })
  ).toBeVisible()
  await expect(page.getByRole("link", { name: "Cart, empty" })).toBeVisible()
})

test("not enough stock comes back from the server, next to the product", async ({
  page,
}) => {
  await addHoney(page, STOCK_LIMIT + 1)
  await page.goto("/checkout")
  await fillCustomer(page)
  await page.getByRole("button", { name: "Place order" }).click()
  await expect(
    page.getByRole("alert").filter({ hasText: "Ilundo Wild Forest Honey" })
  ).toHaveText(
    `Ilundo Wild Forest Honey: Not enough stock: ${STOCK_LIMIT} available.`
  )
  await expect(page).toHaveURL("/checkout")
})

test("empty required fields are named and no order is placed", async ({
  page,
}) => {
  await addHoney(page, 1)
  await page.goto("/checkout")
  await page.getByRole("button", { name: "Place order" }).click()
  await expect(page.getByText("Please fill this in.")).toHaveCount(6)
  await expect(page.getByLabel("Email")).toHaveAttribute("aria-invalid", "true")
  await expect(page).toHaveURL("/checkout")
})

test("with no payment provider configured, the order page says so calmly", async ({
  page,
}) => {
  await addHoney(page, 1)
  await page.goto("/checkout")
  await fillCustomer(page, NO_PROVIDER_EMAIL)
  await page.getByRole("button", { name: "Place order" }).click()
  await page.getByRole("button", { name: "Pay now" }).click()
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Online payment isn't available yet" })
  ).toBeVisible()
  await expect(page.getByRole("button", { name: "Pay now" })).toBeDisabled()
  await expectNoHorizontalScroll(page)
})

test("an empty cart says so", async ({ page }) => {
  await page.goto("/cart")
  await expect(page.getByText("Your cart is empty.")).toBeVisible()
  await page.goto("/checkout")
  await expect(page.getByText("Your cart is empty.")).toBeVisible()
})

test.describe("unknown order", () => {
  test.use({
    allow: {
      responses: [
        { url: "/orders/00000000-0000-4000-8000-000000000000", status: 404 },
      ],
    },
  })

  test("is a real 404", async ({ page }) => {
    const response = await page.goto(
      "/orders/00000000-0000-4000-8000-000000000000"
    )
    expect(response?.status()).toBe(404)
  })
})
