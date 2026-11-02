import { expect, openMenu, paged, test } from "./fake-api"

test.describe(() => {
  test.use({
    allow: { responses: [{ url: "/v1/workspace/w1/products", status: 401 }] },
  })

  test("a token the API refuses ends the session and the login page says so", async ({
    page,
    api,
  }) => {
    api.on("GET", "/workspace/:ws/products", () => ({
      status: 401,
      body: { message: "Unauthenticated." },
    }))
    await page.goto("/products")
    await expect(page).toHaveURL(/\/login$/)
    await expect(
      page
        .getByRole("status")
        .filter({ hasText: "Your session ended. Log in again to carry on." })
    ).toBeVisible()
  })
})

test("a normal logout does not say the session ended", async ({
  page,
  api,
}) => {
  api.on("GET", "/workspace/:ws/products", () => paged([]))
  await page.goto("/products")
  await openMenu(page)
  await page.getByRole("button", { name: "Log out" }).click()
  await expect(page).toHaveURL(/\/login$/)
  await expect(page.getByText("Your session ended.")).toHaveCount(0)
})
