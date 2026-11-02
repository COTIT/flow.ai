import type { Page } from "@playwright/test"

import {
  NO_PROVIDER_EMAIL,
  PRODUCT,
  STOCK_LIMIT,
  VARIANT,
} from "../../mock/fixtures.mjs"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { placeOrder } from "./orders"

// Each test drives one stage and starts from seeded state for the rest: on a loaded
// machine the whole purchase in one test overran its budget at whatever step it was on.

/** Starts the browser with `quantity` honey 500 g jars in the cart (`genesy.cart`, once). */
async function seedCart(page: Page, quantity: number) {
  const cart = {
    state: {
      lines: [
        {
          productId: PRODUCT.honey,
          variantId: VARIANT.honey500,
          quantity,
          name: "Ilundo Wild Forest Honey",
          variantLabel: "Size: 500 g",
          priceGross: 2499,
          currency: "USD",
          decimals: 2,
        },
      ],
    },
    version: 0,
  }
  // Only when absent, so a cart the app empties stays empty on the next page load.
  await page.addInitScript((v) => {
    if (!localStorage.getItem("genesy.cart"))
      localStorage.setItem("genesy.cart", v)
  }, JSON.stringify(cart))
}

async function fillCustomer(page: Page, email = "buyer@e2e.test") {
  await page.getByLabel("Email").fill(email)
  await page.getByLabel("Full name").fill("Ada Buyer")
  await page.getByLabel("Address", { exact: true }).fill("1 Main St")
  await page.getByLabel("Postal code").fill("10001")
  await page.getByLabel("City").fill("New York")
  await page.getByLabel("Country").fill("US")
}

test("add to cart from the product page, then adjust it in the cart", async ({
  page,
}) => {
  await page.goto(`/products/${PRODUCT.honey}`)
  // Enabled only once hydrated (see AddToCart), so these never act on inert server HTML.
  await page.getByLabel("Size or option").selectOption({ label: "Size: 500 g" })
  await page.getByLabel("Quantity").fill("2")
  await page.getByRole("button", { name: "Add to cart" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "Added to your cart." })
  ).toBeVisible()
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
  await expect(page.getByRole("link", { name: "Go to checkout" })).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test("checkout places the order and shows the server's totals", async ({
  page,
}) => {
  await seedCart(page, 3)
  await page.goto("/checkout")
  await fillCustomer(page)
  await expectNoHorizontalScroll(page)
  await page.getByRole("button", { name: "Place order" }).click()
  // Server action -> order POST -> redirect: wait on the navigation itself.
  await page.waitForURL(/\/orders\/[0-9a-f-]{36}$/)

  await expect(
    page.getByRole("heading", { level: 1, name: /^Order WEB-\d+$/ })
  ).toBeVisible()
  await expect(page.getByText("Waiting for payment.")).toBeVisible()
  await expect(page.locator("dd").last()).toHaveText("$74.97")
  await expectNoHorizontalScroll(page)
})

test("paying returns to the order page, thanks the buyer and empties the cart", async ({
  page,
  request,
}) => {
  const id = await placeOrder(request)
  await seedCart(page, 1)
  await page.goto(`/orders/${id}`)
  await expect(page.getByRole("link", { name: "Cart, 1 item" })).toBeVisible()

  // Pay -> provider stand-in -> 302 back here: the URL ends where it started, so wait on
  // the returning page load itself, not on the URL.
  const back = page.waitForResponse(
    (r) =>
      r.request().isNavigationRequest() && r.url().endsWith(`/orders/${id}`)
  )
  await page.getByRole("button", { name: "Pay now" }).click()
  await back
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
  await seedCart(page, STOCK_LIMIT + 1)
  await page.goto("/checkout")
  await fillCustomer(page)
  await page.getByRole("button", { name: "Place order" }).click()
  const error = page
    .getByRole("alert")
    .filter({ hasText: "Please check your order" })
  await expect(error).toContainText(
    `Ilundo Wild Forest Honey: Not enough stock: ${STOCK_LIMIT} available.`
  )
  await expect(page).toHaveURL("/checkout")
})

test("empty required fields are named and no order is placed", async ({
  page,
}) => {
  await seedCart(page, 1)
  await page.goto("/checkout")
  await page.getByRole("button", { name: "Place order" }).click()
  await expect(page.getByText("Please fill this in.")).toHaveCount(6)
  await expect(page.getByLabel("Email")).toHaveAttribute("aria-invalid", "true")
  await expect(page).toHaveURL("/checkout")
})

test("with no payment provider configured, the order page says so calmly", async ({
  page,
  request,
}) => {
  await page.goto(`/orders/${await placeOrder(request, NO_PROVIDER_EMAIL)}`)
  await page.getByRole("button", { name: "Pay now" }).click()
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Online payment isn't available yet" })
  ).toBeVisible()
  await expect(page.getByRole("button", { name: "Pay now" })).toBeDisabled()
  await expectNoHorizontalScroll(page)
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
