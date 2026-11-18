import type { Locator, Page } from "@playwright/test"

import { BASKET, PRINTS, PRODUCT } from "../../mock/fixtures.mjs"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"

// AFM's gift baskets: two options (13 colours x 9 spices). One picker per option, labels that
// fit their buttons, and a price and cart line that follow the choice.

/** Option buttons whose text runs past their own edges. */
const spilling = (group: Locator) =>
  group
    .getByRole("button")
    .evaluateAll(
      (buttons) =>
        buttons.filter((b) => b.scrollWidth > b.clientWidth + 1).length
    )

const price = (page: Page) => page.getByTestId("buy-price")

test("a two-option product has one picker per option, and the choice sets price and cart line", async ({
  page,
}) => {
  await page.goto(`/products/${PRODUCT.basket}`)
  const colour = page.getByRole("group", { name: /^Basket colour/ })
  const spice = page.getByRole("group", { name: /^Spice/ })
  await expect(colour.getByRole("button")).toHaveCount(BASKET.colours.length)
  await expect(spice.getByRole("button")).toHaveCount(BASKET.spices.length)
  // The first variant starts chosen, and each label names its current value.
  await expect(colour).toHaveAccessibleName(
    `Basket colour: ${BASKET.colours[0]}`
  )
  await expect(spice).toHaveAccessibleName(`Spice: ${BASKET.spices[0]}`)
  await expect(price(page)).toHaveText("$74.99")
  await expectNoHorizontalScroll(page)
  expect(await spilling(colour)).toBe(0)
  expect(await spilling(spice)).toBe(0)

  // A real pointer click on each picker.
  const [dearColour, dearSpice] = BASKET.dearer
  await colour.getByRole("button", { name: dearColour, exact: true }).click()
  await spice.getByRole("button", { name: dearSpice, exact: true }).click()
  await expect(
    colour.getByRole("button", { name: dearColour, exact: true })
  ).toHaveAttribute("aria-pressed", "true")
  await expect(colour.locator('[aria-pressed="true"]')).toHaveCount(1)
  await expect(spice).toHaveAccessibleName(`Spice: ${dearSpice}`)
  await expect(price(page)).toHaveText("$84.99")

  await page.getByRole("button", { name: "Add to cart" }).click()
  await expect(
    page
      .locator('[data-variant="success"]')
      .filter({ hasText: "Added to your cart." })
  ).toBeVisible()
  await page.goto("/cart")
  await expect(
    page.getByText(`Basket colour: ${dearColour}, Spice: ${dearSpice}`)
  ).toBeVisible()
})

test("a combination that doesn't exist lands on a real variant", async ({
  page,
}) => {
  await page.goto(`/products/${PRODUCT.basket}`)
  const colour = page.getByRole("group", { name: /^Basket colour/ })
  const spice = page.getByRole("group", { name: /^Spice/ })
  const [lastColour, lastSpice] = BASKET.missing
  await spice.getByRole("button", { name: lastSpice, exact: true }).click()
  await colour.getByRole("button", { name: lastColour, exact: true }).click()
  // That pair isn't sold: the colour holds, the spice moves to the first one sold with it.
  await expect(colour).toHaveAccessibleName(`Basket colour: ${lastColour}`)
  await expect(spice).toHaveAccessibleName(`Spice: ${BASKET.spices[0]}`)
  await expect(spice.locator('[aria-pressed="true"]')).toHaveCount(1)
  await expect(page.getByRole("button", { name: "Add to cart" })).toBeEnabled()
})

test("a one-option product keeps one picker, with long values inside their buttons", async ({
  page,
}) => {
  await page.goto(`/products/${PRODUCT.prints}`)
  const print = page.getByRole("group", { name: /^Print/ })
  await expect(print.getByRole("button")).toHaveCount(PRINTS.length)
  await expectNoHorizontalScroll(page)
  expect(await spilling(print)).toBe(0)
  await print.getByRole("button", { name: new RegExp(`^${PRINTS[0]}`) }).click()
  await expect(
    print.getByRole("button", { name: new RegExp(`^${PRINTS[0]}`) })
  ).toHaveAttribute("aria-pressed", "true")
  await expect(price(page)).toHaveText("$20.99")
})
