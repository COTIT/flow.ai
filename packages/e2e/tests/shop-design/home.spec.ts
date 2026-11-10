import design from "../../../../apps/api/tests/fixtures/storefront-design-v1.json" with { type: "json" }
import { LOT, MOCK_ORIGIN, PRODUCT } from "../../mock/fixtures.mjs"
import {
  expect,
  expectNoHorizontalScroll,
  expectTapTargets,
  test,
} from "../../src/test"

test.beforeEach(async ({ request }) => {
  const response = await request.post(`${MOCK_ORIGIN}/__design`, {
    data: { mode: "published" },
  })
  expect(response.ok()).toBe(true)
})

test("published home keeps responsive original blocks and native trace lookup", async ({
  page,
}, testInfo) => {
  await page.goto("/")
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(
    design.sections[0]!.props.heading
  )
  await expect(
    page.getByRole("heading", { name: design.sections[1]!.props.heading })
  ).toBeVisible()
  await expect(
    page.getByRole("link", { name: /Ilundo Wild Forest Honey/ })
  ).toBeVisible()
  await expect(page.getByLabel("Batch code")).toBeVisible()
  await expectNoHorizontalScroll(page)
  await expectTapTargets(page, { within: "main" })
  await page.screenshot({
    path: testInfo.outputPath("published-home.png"),
    fullPage: true,
  })
  await page.getByLabel("Batch code").fill(LOT.farmed)
  await page.getByRole("button", { name: "Show its passport" }).click()
  await expect(page).toHaveURL(`/b/${LOT.farmed}`)
})

test("published hero call to action opens the shop", async ({ page }) => {
  await page.goto("/")
  await page
    .getByRole("region", { name: design.sections[0]!.props.heading })
    .getByRole("link", { name: "Shop the collection" })
    .click()
  await expect(page).toHaveURL("/shop")
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test("published product links retain the existing purchase path", async ({
  page,
}) => {
  await page.goto("/")
  await page.getByRole("link", { name: /Ilundo Wild Forest Honey/ }).click()
  await expect(page).toHaveURL(`/products/${PRODUCT.honey}`)
  await page.getByRole("button", { name: "Add to cart" }).click()
  await page.getByRole("link", { name: "View cart" }).click()
  await page.getByRole("link", { name: "Go to checkout" }).click()
  await expect(
    page.getByRole("heading", { level: 1, name: "Checkout" })
  ).toBeVisible()
  await expect(page.getByLabel("Email")).toBeVisible()
  await expectNoHorizontalScroll(page)
})

for (const mode of ["missing", "invalid", "unavailable"]) {
  test(`${mode} design keeps the complete legacy home`, async ({
    page,
    request,
  }) => {
    const response = await request.post(`${MOCK_ORIGIN}/__design`, {
      data: { mode },
    })
    expect(response.ok()).toBe(true)
    await page.goto("/")
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(
      "Rooted in African soil. Shared with the world."
    )
    await expect(
      page.getByRole("heading", { name: "What you can check yourself" })
    ).toBeVisible()
    await expect(
      page.getByRole("heading", { name: "Shop by category" })
    ).toBeVisible()
    await expect(
      page.getByRole("heading", { name: "The people behind it" })
    ).toBeVisible()
    await expect(page.getByLabel("Batch code")).toBeVisible()
    await expectNoHorizontalScroll(page)
  })
}

test("legacy catalog redirect precedes the new lookup", async ({ page }) => {
  await page.goto("/?page=2")
  await expect(page).toHaveURL("/shop?page=2")
})
