import { expect, expectNoHorizontalScroll, test } from "../../src/test"

test("home page answers with the promise as its heading", async ({ page }) => {
  const response = await page.goto("/")
  expect(response?.status()).toBe(200)
  await expect(page).toHaveTitle(/Genesy/)
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(
    "Every jar has a story. Every farmer gets their share."
  )
  await expect(page.locator("html")).toHaveAttribute("lang", "en")
  await expectNoHorizontalScroll(page)
})
