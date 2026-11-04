import type { Locator } from "@playwright/test"

import {
  expect,
  expectNoHorizontalScroll,
  expectNoLayoutShift,
  expectNoMotion,
  expectTapTargets,
  test,
} from "../../src/test"

// Mock `ShopCatalogue` in docs/design/web-redesign/canvas: values below are copied from it.
const HEADLINE = "Honey and coffee you can trace to the farmer"

const css = (l: Locator, prop: string) =>
  l.evaluate((el, p) => getComputedStyle(el).getPropertyValue(p), prop)

test("the catalogue is the glass page from the mockup", async ({
  page,
  isMobile,
}) => {
  await page.goto("/")
  await expect(page.locator("body")).toHaveClass(/\btheme-glass\b/)

  const header = page.getByRole("banner")
  expect(await css(header, "border-top-left-radius")).toBe("22px")
  // Header, page and footer share one 1200 px column.
  expect(await css(header.locator(".."), "max-width")).toBe("1200px")
  await expect(header.getByRole("link", { name: "Cart, empty" })).toBeVisible()

  const h1 = page.getByRole("heading", { level: 1 })
  await expect(h1).toHaveAccessibleName(HEADLINE)
  expect(await css(h1, "font-size")).toBe("34px")
  await expect(page.getByText("AFM Honey · Iringa, Tanzania")).toBeVisible()
  await expect(page.getByText("Scan the QR on any jar")).toBeVisible()

  await expect(
    page.getByRole("heading", { level: 2, name: "All products" })
  ).toBeVisible()
  await expect(page.getByText("3 of 4", { exact: true })).toBeVisible()
  const grid = page.getByRole("list", { name: "All products" })
  const columns = (await css(grid, "grid-template-columns")).split(" ")
  expect(columns).toHaveLength(isMobile ? 2 : 3)

  const honey = page.getByRole("link", { name: /Ilundo Wild Forest Honey/ })
  await expect(honey).toContainText("$14.99")
  // stock_status "low": gold "Few left"; "sold_out": slate "Sold out".
  await expect(honey.locator('[data-slot="badge"]')).toHaveText("Few left")
  await expect(honey.locator('[data-slot="badge"]')).toHaveAttribute(
    "data-variant",
    "gold"
  )
  // The link names the product; its photo is decorative there.
  await expect(honey.locator('img[alt=""]')).toBeVisible()
  await expect(page.getByRole("link", { name: /Beeswax Block/ })).toContainText(
    "Sold out"
  )

  const footer = page.getByRole("contentinfo")
  await expect(footer).toContainText("Traceable from hive to kitchen")
  await expect(footer).toContainText(
    "Shipping to the United States. Prices include VAT."
  )
  expect(await css(footer, "border-top-left-radius")).toBe("24px")
  // Phones have no language picker in the header: the footer one is there for them.
  await expect(footer.getByLabel("Language")).toBeVisible()
  await expect(header.getByLabel("Language")).toBeVisible({
    visible: !isMobile,
  })

  await expectNoHorizontalScroll(page)
  if (isMobile) await expectTapTargets(page)
  await expectNoLayoutShift(page)
})

test("the server sends the headline and products (search engines see them)", async ({
  request,
}) => {
  const html = await (await request.get("/")).text()
  expect(html).toContain(HEADLINE)
  expect(html).toContain("Ilundo Wild Forest Honey")
})

test("with motion, the headline comes into focus; photos glare only under a mouse", async ({
  page,
  isMobile,
}) => {
  await page.goto("/")
  const h1 = page.getByRole("heading", { level: 1 })
  await expect(h1.locator("[data-motion]")).toHaveCount(1)
  await expect(h1).toHaveAccessibleName(HEADLINE)
  const honey = page.getByRole("link", { name: /Ilundo Wild Forest Honey/ })
  await expect(honey.locator("[data-motion]")).toHaveCount(isMobile ? 0 : 1)
  await expectNoLayoutShift(page)
})

test.describe("with reduced motion", () => {
  test.use({ reducedMotion: "reduce" })

  test("nothing animates and the words stay as written", async ({ page }) => {
    await page.goto("/")
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(HEADLINE)
    await expect(
      page.getByRole("link", { name: /Ilundo Wild Forest Honey/ })
    ).toBeVisible()
    await expectNoMotion(page)
  })
})
