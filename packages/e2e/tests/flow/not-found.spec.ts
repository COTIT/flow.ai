import { expect, expectNoHorizontalScroll, test } from "../../src/test"

test.describe(() => {
  test.use({ allow: { responses: [{ url: "/no-such-page", status: 404 }] } })

  test("an unknown address says so and links back to the start", async ({
    page,
  }) => {
    const response = await page.goto("/no-such-page")
    expect(response!.status()).toBe(404)
    await expect(
      page.getByRole("heading", { name: "This page does not exist" })
    ).toBeVisible()
    await page.getByRole("link", { name: "Go to the start" }).click()
    await expect(page).toHaveURL("/login")
    await expectNoHorizontalScroll(page)
  })
})
