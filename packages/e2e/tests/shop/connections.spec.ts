import { PRODUCT, SHOP_CONNECTIONS } from "../../mock/fixtures.mjs"
import { expect, test } from "../../src/test"

test("workspace catalog, carts and checkout keep their connection across two tabs", async ({
  page,
  context,
}) => {
  const first = `/s/${SHOP_CONNECTIONS[0]}`
  const second = `/s/${SHOP_CONNECTIONS[1]}`
  await page.goto(`${first}/products/${PRODUCT.honey}`)
  await expect(page.getByRole("banner")).toContainText("First shop")
  await page.getByRole("button", { name: "Add to cart", exact: true }).click()
  await page.getByRole("link", { name: "View cart", exact: true }).click()
  await expect(page).toHaveURL(`${first}/cart`)
  await expect(page.getByRole("link", { name: "Cart, 1 item" })).toBeVisible()
  const other = await context.newPage()
  await other.goto(`${second}/cart`)
  await expect(other.getByRole("banner")).toContainText("Second shop")
  await expect(other.getByRole("link", { name: "Cart, empty" })).toBeVisible()
  await expect(page.getByRole("link", { name: "Cart, 1 item" })).toBeVisible()
  await page.getByRole("link", { name: "Go to checkout", exact: true }).click()
  await expect(page).toHaveURL(`${first}/checkout`)
  await page.getByLabel("Email", { exact: true }).fill("scoped@e2e.test")
  await page.getByLabel("Full name").fill("Ada Buyer")
  await page.getByLabel("Address", { exact: true }).fill("1 Main St")
  await page.getByLabel("Postal code").fill("10001")
  await page.getByLabel("City").fill("New York")
  await page.getByLabel("Country").fill("US")
  await page.getByRole("button", { name: "Place order", exact: true }).click()
  await page.waitForURL(
    new RegExp(`/s/${SHOP_CONNECTIONS[0]}/orders/[0-9a-f-]{36}$`)
  )
  await page.getByRole("button", { name: /^Pay \$/ }).click()
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Thank you! Your payment is in" })
  ).toBeVisible()
  await expect(page).toHaveURL(
    new RegExp(`/s/${SHOP_CONNECTIONS[0]}/orders/[0-9a-f-]{36}$`)
  )
  await expect(page.getByRole("link", { name: "Cart, empty" })).toBeVisible()
  await other.close()
})
