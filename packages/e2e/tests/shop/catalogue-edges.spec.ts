import type { Page } from "@playwright/test"

import {
  FAILING_PAGE,
  LONG_NAME,
  LOT,
  PRODUCT,
  SECRET_BODY,
} from "../../mock/fixtures.mjs"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"

const meta = (page: Page, selector: string) =>
  page.locator(`head ${selector}`).first()

test("the pager walks the catalogue page by page", async ({ page }) => {
  await page.goto("/shop")
  const pager = page.getByRole("navigation", { name: "Catalog pages" })
  await expect(pager).toContainText("Page 1 of 2")
  await expect(pager.getByRole("button", { name: "Previous" })).toBeDisabled()
  await expect(page.getByText("18 of 19", { exact: true })).toBeVisible()

  await pager.getByRole("link", { name: "Next" }).click()
  await expect(page).toHaveURL("/shop?page=2", { timeout: 15_000 })
  await expect(pager).toContainText("Page 2 of 2")
  await expect(pager.getByRole("button", { name: "Next" })).toBeDisabled()
  await expect(page.getByRole("link", { name: LONG_NAME })).toBeVisible()
  await expect(
    page.getByRole("link", { name: /Ilundo Wild Forest Honey/ })
  ).toHaveCount(0)

  await pager.getByRole("link", { name: "Previous" }).click()
  await expect(page).toHaveURL("/shop?page=1", { timeout: 15_000 })
  await expect(
    page.getByRole("link", { name: /Ilundo Wild Forest Honey/ })
  ).toBeVisible()
})

test("an old catalogue page link lands on the same page of the shop", async ({
  page,
}) => {
  await page.goto("/?page=2")
  await expect(page).toHaveURL("/shop?page=2")
  await expect(page.getByRole("link", { name: LONG_NAME })).toBeVisible()
})

test("a very long product name wraps instead of pushing the page wide", async ({
  page,
}) => {
  await page.goto("/shop?page=2")
  const card = page.getByRole("link", { name: LONG_NAME })
  await expect(card).toContainText("$32.00")
  await expect(card).toContainText("In stock")
  await expectNoHorizontalScroll(page)

  await card.click()
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(LONG_NAME)
  await expectNoHorizontalScroll(page)
  await page.getByRole("button", { name: "Add to cart" }).click()
  await page.getByRole("link", { name: "View cart" }).click()
  await expect(
    page.getByRole("list", { name: "Your cart" }).getByText(LONG_NAME)
  ).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test("the product page shows the few-left badge, and the size's price once chosen", async ({
  page,
}) => {
  await page.goto(`/products/${PRODUCT.honey}`)
  await expect(page.getByText("Few left", { exact: true })).toBeVisible()
  // The big price comes before the size buttons in reading order.
  const price = page.getByText(/^\$\d+\.\d\d$/).first()
  await expect(price).toHaveText("$14.99")
  await page.getByRole("button", { name: "500 g $24.99" }).click()
  await expect(
    page.getByRole("button", { name: "500 g $24.99" })
  ).toHaveAttribute("aria-pressed", "true")
  await expect(price).toHaveText("$24.99")
})

test("a product whose batch passport is unpublished shows no passport card", async ({
  page,
}) => {
  await page.goto(`/products/${PRODUCT.wax}`)
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(
    "Beeswax Block"
  )
  await expect(
    page.getByRole("heading", { name: "This jar's passport" })
  ).toHaveCount(0)
  await expect(
    page.getByRole("link", { name: "Open the passport" })
  ).toHaveCount(0)
})

test.describe("search engines", () => {
  test("the catalogue has the shop's title and description", async ({
    page,
  }) => {
    await page.goto("/")
    await expect(page).toHaveTitle("African Farmers Market")
    await expect(meta(page, 'meta[name="description"]')).toHaveAttribute(
      "content",
      /^Rooted in African soil\. Shared with the world\./
    )
    await page.goto("/shop")
    await expect(page).toHaveTitle("Shop · African Farmers Market")
  })

  test("a product has its description, canonical link and share card", async ({
    page,
    baseURL,
  }) => {
    await page.goto(`/products/${PRODUCT.honey}`)
    const url = `${baseURL}/products/${PRODUCT.honey}`
    const description =
      "Raw honey from the miombo woodlands of Iringa, Tanzania."
    await expect(meta(page, 'meta[name="description"]')).toHaveAttribute(
      "content",
      description
    )
    await expect(meta(page, 'link[rel="canonical"]')).toHaveAttribute(
      "href",
      url
    )
    await expect(meta(page, 'meta[property="og:title"]')).toHaveAttribute(
      "content",
      /Ilundo Wild Forest Honey/
    )
    await expect(meta(page, 'meta[property="og:url"]')).toHaveAttribute(
      "content",
      url
    )
    await expect(meta(page, 'meta[property="og:description"]')).toHaveAttribute(
      "content",
      description
    )
  })

  test("a batch passport is an article with its own canonical link", async ({
    page,
    baseURL,
  }) => {
    await page.goto(`/b/${LOT.released}`)
    await expect(page).toHaveTitle(
      "Wild forest honey, harvest July 2026 · African Farmers Market"
    )
    await expect(meta(page, 'meta[name="description"]')).toHaveAttribute(
      "content",
      /^This jar was filled from one harvest/
    )
    await expect(meta(page, 'link[rel="canonical"]')).toHaveAttribute(
      "href",
      `${baseURL}/b/${LOT.released}`
    )
    await expect(meta(page, 'meta[property="og:type"]')).toHaveAttribute(
      "content",
      "article"
    )
  })
})

test.describe("addresses that are not ids", () => {
  test.use({
    allow: {
      responses: [
        { url: "/products/abc", status: 404 },
        { url: "/orders/abc", status: 404 },
      ],
    },
  })

  for (const path of ["/products/abc", "/orders/abc"])
    test(`${path} is a real 404`, async ({ page }) => {
      const response = await page.goto(path)
      expect(response?.status()).toBe(404)
      await expect(
        page.getByRole("heading", { name: "Page not found" })
      ).toBeVisible()
    })
})

test.describe("catalogue API down", () => {
  const path = `/shop?page=${FAILING_PAGE}`
  test.use({
    allow: {
      responses: [{ url: path, status: 500 }],
      console: [
        "Error: Minified React error #441; visit https://react.dev/errors/441 for the full message or use the non-minified dev environment for full errors and additional helpful warnings.",
      ],
    },
  })

  test("shows the friendly error page and nothing from upstream", async ({
    page,
  }) => {
    const response = await page.goto(path)
    expect(response?.status()).toBe(500)
    expect(await response!.text()).not.toContain(SECRET_BODY)
    const notice = page
      .getByRole("alert")
      .filter({ hasText: "The shop is taking a break" })
    await expect(
      notice.getByRole("button", { name: "Try again" })
    ).toBeVisible()
    await expectNoHorizontalScroll(page)
  })
})
