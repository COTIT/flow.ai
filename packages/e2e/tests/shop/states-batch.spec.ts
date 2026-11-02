import { LOT } from "../../mock/fixtures.mjs"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"

test.describe("a batch code that opens no passport", () => {
  test.use({
    allow: { responses: [{ url: `/b/${LOT.unknown}`, status: 404 }] },
  })

  test("explains why (mistyped or not published) and offers the shop", async ({
    page,
  }) => {
    const response = await page.goto(`/b/${LOT.unknown}`)
    expect(response?.status()).toBe(404)
    await expect(
      page.getByRole("heading", { level: 1, name: "We can't find this batch" })
    ).toBeVisible()
    await expect(
      page.getByText(/its passport may not be published yet/)
    ).toBeVisible()
    await expectNoHorizontalScroll(page)
    await page.getByRole("link", { name: "Browse the shop" }).click()
    await expect(page).toHaveURL("/")
  })
})
