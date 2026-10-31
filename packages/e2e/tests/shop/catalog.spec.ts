import { PRODUCT } from "../../mock/fixtures.mjs"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"

test("catalog lists products with prices and stock, and opens a product", async ({
  page,
}) => {
  await page.goto("/")
  await expect(
    page.getByRole("heading", { level: 1, name: "Shop" })
  ).toBeVisible()

  const honey = page.getByRole("link", { name: /Ilundo Wild Forest Honey/ })
  await expect(honey).toContainText("$14.99")
  await expect(honey).toContainText("In stock")
  await expect(page.getByRole("link", { name: /Beeswax Block/ })).toContainText(
    "Sold out"
  )
  await expectNoHorizontalScroll(page)

  await honey.click()
  await expect(page).toHaveURL(`/products/${PRODUCT.honey}`)
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(
    "Ilundo Wild Forest Honey"
  )
  await expect(page.getByText("Size: 500 g")).toBeVisible()
  await expect(page.getByText("$24.99")).toBeVisible()
  await expect(page).toHaveTitle("Ilundo Wild Forest Honey · Genesy Shop")
  await expectNoHorizontalScroll(page)

  await page.getByRole("link", { name: "Back to the shop" }).click()
  await expect(page).toHaveURL("/")
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
