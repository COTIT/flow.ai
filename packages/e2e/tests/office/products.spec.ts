import { mockApi } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { officeApi } from "./api"

test.beforeEach(async ({ page }) => {
  await seedSession(page)
})

test("lists the workspace's products", async ({ page }) => {
  await mockApi(page, officeApi())
  await page.goto("/products")
  await expect(page.getByRole("img", { name: "Genesy" })).toBeVisible()
  await expect(page.getByRole("heading", { name: "Products" })).toBeVisible()
  await expect(page.getByRole("cell", { name: "Beeswax Block" })).toBeVisible()
  await expect(page.getByRole("cell", { name: "14.99" })).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test("a product with a picture shows its thumbnail; one without keeps the icon tile", async ({
  page,
}) => {
  // A data URL keeps the picture inside the office CSP in tests (no storage host here).
  const pixel = "data:image/gif;base64,R0lGODlhAQABAAAAACw="
  const product = { product_nr: "P-001", reference: null, price_incl: "9.00", status: "active" }
  await mockApi(
    page,
    officeApi([
      {
        ...product,
        id: "p1",
        name: "Ilundo Wild Forest Honey",
        thumbnail: { url: pixel, expires_at: "2099-01-01T00:00:00Z" },
      },
      { ...product, id: "p2", name: "Beeswax Block", thumbnail: null },
    ])
  )
  await page.goto("/products")
  const row = (name: string) => page.getByRole("row").filter({ hasText: name })
  await expect(row("Ilundo Wild Forest Honey").locator(`img[src="${pixel}"]`)).toHaveCount(1)
  await expect(row("Beeswax Block").locator("img")).toHaveCount(0)
  await expect(row("Beeswax Block").locator("svg")).toHaveCount(1)
  await expectNoHorizontalScroll(page)
})

test("says so when there are no products", async ({ page }) => {
  await mockApi(page, officeApi([]))
  await page.goto("/products")
  await expect(page.getByText("No products yet.")).toBeVisible()
})

test.describe("API error", () => {
  test.use({
    allow: { responses: [{ url: "/v1/workspace/w1/products", status: 500 }] },
  })

  test("shows a message instead of the table", async ({ page }) => {
    await mockApi(page, {
      ...officeApi(),
      "GET /workspace/w1/products": { status: 500, body: { message: "boom" } },
    })
    await page.goto("/products")
    // Waits out TanStack Query's three retries; the API's own words show, with a retry.
    await expect(
      page.getByRole("alert").filter({ hasText: "boom" })
    ).toBeVisible({ timeout: 15_000 })
  })
})
