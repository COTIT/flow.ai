import { PRODUCT } from "../../mock/fixtures.mjs"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"

test("an empty cart says why and offers the shop", async ({ page }) => {
  await page.goto("/cart")
  await expect(page.getByText("Your cart is empty")).toBeVisible()
  await expect(
    page.getByText("Add something from the shop and it will show up here.")
  ).toBeVisible()
  await expectNoHorizontalScroll(page)
  await page.getByRole("link", { name: "Browse the shop" }).click()
  await expect(page).toHaveURL("/shop")
})

test("checkout with an empty cart sends you to the shop", async ({ page }) => {
  await page.goto("/checkout")
  await expect(page.getByText("Your cart is empty")).toBeVisible()
  await expect(
    page.getByRole("link", { name: "Browse the shop" })
  ).toBeVisible()
})

test("a catalog page past the end is empty, with a way back", async ({
  page,
}) => {
  await page.goto("/shop?page=9")
  await expect(page.getByText("Nothing on this page")).toBeVisible()
  await expectNoHorizontalScroll(page)
  await page.getByRole("link", { name: "Go to the first page" }).click()
  await expect(
    page.getByRole("link", { name: /Ilundo Wild Forest Honey/ })
  ).toBeVisible()
})

test("a sold-out product says so in words, and can't be added", async ({
  page,
}) => {
  await page.goto(`/products/${PRODUCT.wax}`)
  const warning = page
    .locator('[data-variant="warning"]')
    .filter({ hasText: "Sold out for now" })
  await expect(warning).toContainText("Check back soon.")
  await expect(page.getByRole("button", { name: "Add to cart" })).toBeDisabled()
  await expectNoHorizontalScroll(page)
})
