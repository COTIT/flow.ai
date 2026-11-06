import { PRODUCT } from "../../mock/fixtures.mjs"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"

test("catalog lists products with prices and stock, and opens a product", async ({
  page,
}) => {
  await page.goto("/shop")
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Shop")

  const honey = page.getByRole("link", { name: /Ilundo Wild Forest Honey/ })
  await expect(honey).toContainText("$14.99")
  await expect(honey).toContainText("Few left")
  await expect(page.getByRole("link", { name: /Beeswax Block/ })).toContainText(
    "Sold out"
  )
  await expectNoHorizontalScroll(page)

  await honey.click()
  // Client navigation; on a loaded machine it can take longer than the 5 s default.
  await expect(page).toHaveURL(`/products/${PRODUCT.honey}`, {
    timeout: 15_000,
  })
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(
    "Ilundo Wild Forest Honey"
  )
  const sizes = page.getByRole("group", { name: "Size" })
  await expect(
    sizes.getByRole("button", { name: "500 g $24.99" })
  ).toBeVisible()
  await expect(page).toHaveTitle(
    "Ilundo Wild Forest Honey · African Farmers Market"
  )
  await expectNoHorizontalScroll(page)

  await page.getByRole("link", { name: "Back to the shop" }).click()
  await expect(page).toHaveURL("/shop")
})

test("a product sold by size shows its default size's price, never $0.00", async ({
  page,
}) => {
  // Product-level price_gross is 0; the default variant (250 g) is $12.00.
  await page.goto("/shop")
  const coffee = page.getByRole("link", { name: /Ruaha Coffee/ })
  await expect(coffee).toContainText("$12.00")
  await expect(coffee).not.toContainText("$0.00")

  await page.goto(`/products/${PRODUCT.coffee}`)
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(
    "Ruaha Coffee"
  )
  const sizes = page.getByRole("group", { name: "Size" })
  await expect(
    sizes.getByRole("button", { name: "250 g $12.00" })
  ).toHaveAttribute("aria-pressed", "true")
  await expect(page.getByText("$12.00", { exact: true }).first()).toBeVisible()
  await expect(page.locator("main")).not.toContainText("$0.00")
  // The server's HTML too, before any script runs.
  const html = await (
    await page.request.get(`/products/${PRODUCT.coffee}`)
  ).text()
  expect(html).not.toContain("$0.00")
  await expectNoHorizontalScroll(page)
})

test.describe("unknown product", () => {
  test.use({
    allow: {
      responses: [
        { url: "/products/00000000-0000-4000-8000-000000000000", status: 404 },
      ],
    },
  })

  test("is a real 404", async ({ page }) => {
    const response = await page.goto(
      "/products/00000000-0000-4000-8000-000000000000"
    )
    expect(response?.status()).toBe(404)
    await expect(
      page.getByRole("heading", { name: "Page not found" })
    ).toBeVisible()
  })
})
