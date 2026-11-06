import { PRODUCT } from "../../mock/fixtures.mjs"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"

// A product keeps its batch link for up to a minute; a recall or hold since then shows first.
test("a product whose batch was recalled says so before anything else", async ({
  page,
}) => {
  await page.goto(`/products/${PRODUCT.recalledHoney}`)
  const alert = page
    .getByRole("alert")
    .filter({ hasText: "This batch has been recalled" })
  await expect(alert).toBeVisible()
  const before = await alert.evaluate(
    (a) =>
      !!(
        a.compareDocumentPosition(document.querySelector("h1")!) &
        Node.DOCUMENT_POSITION_FOLLOWING
      )
  )
  expect(before, "recall banner before the product name").toBe(true)
  await expect(
    page
      .getByRole("region", { name: "Where it came from" })
      .getByText("Recalled", { exact: true })
  ).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test("a product whose batch is on hold shows a warning, not a recall", async ({
  page,
}) => {
  await page.goto(`/products/${PRODUCT.heldTea}`)
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "This batch is on hold while it is being checked." })
  ).toBeVisible()
  await expect(page.getByText(/has been recalled/)).toHaveCount(0)
})

test("a released batch adds no warning", async ({ page }) => {
  await page.goto(`/products/${PRODUCT.honey}`)
  await expect(
    page.getByRole("region", { name: "Where it came from" })
  ).toBeVisible()
  await expect(page.locator("main").getByRole("alert")).toHaveCount(0)
  await expect(page.locator('main [data-variant="warning"]')).toHaveCount(0)
})
