import { mockApi } from "../../src/api-mock"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { officeApi } from "./api"

test("logging in opens the overview", async ({ page }) => {
  await mockApi(page, {
    ...officeApi(),
    "POST /login": {
      body: {
        data: {
          access_token: "e2e-token",
          token_type: "Bearer",
          expires_at: "2099-01-01T00:00:00Z",
          workspace_ids: ["w1"],
        },
      },
    },
  })
  await page.goto("/login")
  await expect(page.getByRole("img", { name: "Genesy" })).toBeVisible()
  await expectNoHorizontalScroll(page)
  await expect(page.getByRole("heading", { name: "Log in" })).toBeVisible()
  await expect(page.getByRole("button", { name: "Language" })).toBeVisible()
  await expect(page.getByRole("button", { name: "Theme" })).toBeVisible()

  await page.getByLabel("Email").fill("e2e@genesy.test")
  await page.getByLabel("Password").fill("e2e-password-only")
  await page.getByRole("button", { name: "Log in" }).click()

  await expect(page).toHaveURL("/")
  await expect(page.getByRole("heading", { level: 1, name: "Overview" })).toBeVisible()
})

test.describe("wrong password", () => {
  test.use({ allow: { responses: [{ url: "/v1/login", status: 401 }] } })

  test("says so and stays on the login page", async ({ page }) => {
    await mockApi(page, {
      "POST /login": { status: 401, body: { message: "Unauthenticated." } },
    })
    await page.goto("/login")
    await page.getByLabel("Email").fill("e2e@genesy.test")
    await page.getByLabel("Password").fill("wrong-password")
    await page.getByRole("button", { name: "Log in" }).click()
    await expect(
      page.getByRole("alert").filter({ hasText: "Wrong email or password." })
    ).toBeVisible()
    await expect(page).toHaveURL("/login")
  })
})

test.describe("network failure", () => {
  test.use({ allow: { failures: ["/v1/login"] } })

  test("/login: a dropped connection says so, keeps the email, and logging in again works", async ({
    page,
  }) => {
    await mockApi(page, {
      ...officeApi(),
      "POST /login": {
        body: {
          data: {
            access_token: "e2e-token",
            token_type: "Bearer",
            expires_at: "2099-01-01T00:00:00Z",
            workspace_ids: ["w1"],
          },
        },
      },
    })
    let fail = true
    await page.route("**/v1/login", async (route) => {
      if (fail) {
        fail = false
        return route.abort("internetdisconnected")
      }
      return route.fallback()
    })
    await page.goto("/login")
    await page.getByLabel("Email").fill("e2e@genesy.test")
    await page.getByLabel("Password").fill("e2e-password-only")
    await page.getByRole("button", { name: "Log in" }).click()
    await expect(
      page.getByRole("alert").filter({ hasText: "Something went wrong. Try again." })
    ).toBeVisible()
    await expect(page).toHaveURL("/login")
    await expect(page.getByLabel("Email")).toHaveValue("e2e@genesy.test")
    await expect(page.getByRole("alert").filter({ hasText: "Wrong email or password." })).toHaveCount(0)
    await page.getByRole("button", { name: "Log in" }).click()
    await expect(page).toHaveURL("/")
  })
})
