import { PRODUCT } from "../../mock/fixtures.mjs"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { seedCart } from "./cart"

const HONEY = "Ilundo Wild Forest Honey"

test("the product page starts at one and can't go below it", async ({
  page,
}) => {
  await page.goto(`/products/${PRODUCT.honey}`)
  const less = page.getByRole("button", { name: `One less ${HONEY}` })
  await expect(page.getByRole("button", { name: "Add to cart" })).toBeEnabled()
  await expect(page.getByLabel("Quantity")).toHaveText("1")
  await expect(less).toBeDisabled()
  await page.getByRole("button", { name: `One more ${HONEY}` }).click()
  await expect(page.getByLabel("Quantity")).toHaveText("2")
  await expect(less).toBeEnabled()
})

test("adding the same size twice makes one line, not two", async ({ page }) => {
  await page.goto(`/products/${PRODUCT.honey}`)
  const add = page.getByRole("button", { name: "Add to cart" })
  await add.click()
  await expect(page.getByRole("link", { name: "Cart, 1 item" })).toBeVisible()
  await add.click()
  await expect(page.getByRole("link", { name: "Cart, 2 items" })).toBeVisible()

  await page.goto("/cart")
  await expect(
    page.getByRole("list", { name: "Your cart" }).getByRole("listitem")
  ).toHaveCount(1)
  await expect(page.getByLabel("Quantity")).toHaveText("2")
})

test("one less at a quantity of one takes the line out of the cart", async ({
  page,
}) => {
  await seedCart(page, 1)
  await page.goto("/cart")
  await page.getByRole("button", { name: `One less ${HONEY}` }).click()
  await expect(page.getByText("Your cart is empty")).toBeVisible()
  await expect(page.getByRole("link", { name: "Cart, empty" })).toBeVisible()
  // Gone for good, not just hidden: a reload shows the empty cart too.
  await page.reload()
  await expect(page.getByText("Your cart is empty")).toBeVisible()
})

test("the bin button removes the line", async ({ page }) => {
  await seedCart(page, 3)
  await page.goto("/cart")
  await page.getByRole("button", { name: `Remove ${HONEY}` }).click()
  await expect(page.getByText("Your cart is empty")).toBeVisible()
  await expect(page.getByRole("link", { name: "Cart, empty" })).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test("a line stops at the checkout's limit of 1000", async ({ page }) => {
  await seedCart(page, 1000)
  await page.goto("/cart")
  await expect(page.getByLabel("Quantity")).toHaveText("1000")
  await expect(
    page.getByRole("button", { name: `One more ${HONEY}` })
  ).toBeDisabled()
  await expect(
    page.getByRole("button", { name: `One less ${HONEY}` })
  ).toBeEnabled()
  await expectNoHorizontalScroll(page)
})
