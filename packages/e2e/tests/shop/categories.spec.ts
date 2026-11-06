import { CATEGORY } from "../../mock/fixtures.mjs"
import {
  expect,
  expectNoHorizontalScroll,
  expectNoRawValues,
  test,
} from "../../src/test"

test("a category tile opens that category; its chip is marked and the rest are a tap away", async ({
  page,
}) => {
  await page.goto("/")
  await page
    .getByRole("region", { name: "Shop by category" })
    .getByRole("link", { name: /^Coffee/ })
    .click()
  await expect(page).toHaveURL(`/shop?category=${CATEGORY.coffee}`, {
    timeout: 15_000,
  })
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Coffee")
  await expect(page).toHaveTitle("Coffee · African Farmers Market")
  await expect(
    page.getByText("Arabica from smallholder farms in Tanzania.")
  ).toBeVisible()

  const chips = page.getByRole("navigation", { name: "Categories" })
  await expect(chips.getByRole("link", { name: "Coffee" })).toHaveAttribute(
    "aria-current",
    "page"
  )
  const grid = page.getByRole("list", { name: "Coffee" })
  await expect(grid.getByRole("listitem")).toHaveCount(3)
  await expect(grid).toContainText("Rift Valley Ground Coffee")
  await expect(grid).not.toContainText("Ilundo Wild Forest Honey")
  // One page in this category: no pager.
  await expect(
    page.getByRole("navigation", { name: "Catalog pages" })
  ).toHaveCount(0)
  await expectNoRawValues(page)
  await expectNoHorizontalScroll(page)

  await chips.getByRole("link", { name: "All" }).click()
  await expect(page).toHaveURL("/shop", { timeout: 15_000 })
  await expect(chips.getByRole("link", { name: "All" })).toHaveAttribute(
    "aria-current",
    "page"
  )
})

test("the chip row scrolls on a phone instead of pushing the page wide", async ({
  page,
  isMobile,
}) => {
  test.skip(!isMobile, "phones only")
  await page.goto(`/shop?category=${CATEGORY.stickers}`)
  const chips = page.getByRole("navigation", { name: "Categories" })
  await expect(chips.getByRole("link", { name: "Stickers" })).toBeVisible()
  await expectNoHorizontalScroll(page)
  // The chosen chip is reachable by keyboard even when scrolled out of view.
  await chips.getByRole("link", { name: "Stickers" }).focus()
  await expect(chips.getByRole("link", { name: "Stickers" })).toBeInViewport()
})

test("a category that no longer exists says so and links to everything", async ({
  page,
}) => {
  await page.goto(`/shop?category=${CATEGORY.unknown}`)
  await expect(page.getByText("We can't find this category")).toBeVisible()
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Shop")
  await expectNoRawValues(page)
  await page.getByRole("link", { name: "See all products" }).click()
  await expect(page).toHaveURL("/shop", { timeout: 15_000 })
})

test("a category that is not an id is ignored, never sent to the API", async ({
  page,
}) => {
  await page.goto("/shop?category=../../orders")
  await expect(page.getByText("We can't find this category")).toBeVisible()
})
