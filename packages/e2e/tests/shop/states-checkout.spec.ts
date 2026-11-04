import type { Page } from "@playwright/test"

import { BUSY_EMAIL, PRODUCT, VARIANT } from "../../mock/fixtures.mjs"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"

test.beforeEach(async ({ page }) => {
  const cart = {
    state: {
      lines: [
        {
          productId: PRODUCT.honey,
          variantId: VARIANT.honey500,
          quantity: 1,
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
  await page.addInitScript((v) => {
    if (!localStorage.getItem("genesy.cart"))
      localStorage.setItem("genesy.cart", v)
  }, JSON.stringify(cart))
})

async function submit(page: Page, email: string) {
  await page.goto("/checkout")
  await page.getByLabel("Email").fill(email)
  await page.getByLabel("Full name").fill("Ada Buyer")
  await page.getByLabel("Address", { exact: true }).fill("1 Main St")
  await page.getByLabel("Postal code").fill("10001")
  await page.getByLabel("City").fill("New York")
  await page.getByLabel("Country").fill("US")
  await page.getByRole("button", { name: "Place order" }).click()
}

test("the API's field error appears under that field, with a general error on top", async ({
  page,
}) => {
  await submit(page, "not-an-email")
  await expect(
    page
      .getByRole("alert")
      .filter({ hasText: "Please check the highlighted fields" })
  ).toBeVisible()
  const email = page.getByLabel("Email")
  await expect(email).toHaveAttribute("aria-invalid", "true")
  // The error first (a translated line, then the API's own words), then the field's hint.
  await expect(email).toHaveAccessibleDescription(
    /^This doesn't look right\. The email must be a valid email address\./
  )
  await expectNoHorizontalScroll(page)
})

test("a busy shop is a calm warning, not an error", async ({ page }) => {
  await submit(page, BUSY_EMAIL)
  const warning = page
    .locator('[data-variant="warning"]')
    .filter({ hasText: "very busy" })
  await expect(warning).toHaveAttribute("role", "status")
  await expect(page).toHaveURL("/checkout")
})
