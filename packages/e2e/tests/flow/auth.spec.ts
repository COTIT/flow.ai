import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import {
  AMINA,
  BARAKA,
  flowApi,
  logIn,
  mockApi,
  readiness,
  RUAHA,
  savedToken,
} from "./api"

test("a visitor who is not signed in is sent to the login page", async ({
  page,
}) => {
  const response = await page.goto("/")
  expect(response!.status()).toBe(200)
  await expect(page).toHaveURL("/login")
  await expect(page.getByRole("heading", { name: "Log in" })).toBeVisible()
  await expect(page.getByText("Talk to your business assistant.")).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test("logging in opens the landing with who and where", async ({ page }) => {
  await mockApi(page, flowApi())
  await readiness(page, 200, { enabled: true, ready: true })
  await page.goto("/login")
  await logIn(page)
  await expect(page).toHaveURL("/")
  await expect(page.getByText(AMINA.name)).toBeVisible()
  await expect(page.getByText(AMINA.email)).toBeVisible()
  await expect(page.getByText("Ilundo Honey Co.")).toBeVisible()
  await expect(page.getByText("The assistant is available")).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test("the button stays disabled while the login is on its way", async ({
  page,
}) => {
  let release = () => {}
  const held = new Promise<void>((resolve) => (release = resolve))
  await mockApi(page, flowApi())
  await readiness(page, 200, { enabled: true, ready: true })
  await page.route("**/v1/login", async (route) => {
    await held
    await route.fulfill({
      contentType: "application/json",
      body: JSON.stringify(flowApi()["POST /login"]!.body),
    })
  })
  await page.goto("/login")
  await logIn(page)
  await expect(page.getByRole("button", { name: "Log in" })).toBeDisabled()
  release()
  await expect(page).toHaveURL("/")
})

for (const [status, text] of [
  [401, "Wrong email or password."],
  [429, "Too many attempts. Wait a few minutes and try again."],
  [422, "Check the email and password fields."],
  [500, "Something went wrong. Try again."],
] as const)
  test.describe(`a ${status} login answer`, () => {
    test.use({ allow: { responses: [{ url: "/v1/login", status }] } })

    test("is said in plain words and stays on the login page", async ({
      page,
    }) => {
      await mockApi(page, {
        "POST /login": { status, body: { message: "refused" } },
      })
      await page.goto("/login")
      await logIn(page)
      await expect(
        page.getByRole("alert").filter({ hasText: text })
      ).toBeVisible()
      await expect(page).toHaveURL("/login")
      expect(await savedToken(page)).toBeNull()
    })
  })

test("logging out forgets the session and shows no earlier data to the next login", async ({
  page,
}) => {
  await mockApi(page, flowApi())
  await readiness(page, 200, { enabled: true, ready: true })
  await page.goto("/login")
  await logIn(page)
  await expect(page.getByText(AMINA.name)).toBeVisible()

  await page.getByRole("button", { name: "Log out" }).click()
  await expect(page).toHaveURL("/login")
  await expect(page.getByText("Your session ended.")).toHaveCount(0)
  expect(await savedToken(page)).toBeNull()

  // The next member's profile is slow: nothing of Amina's may fill the gap.
  let release = () => {}
  const held = new Promise<void>((resolve) => (release = resolve))
  await mockApi(page, {
    ...flowApi({
      "GET /me/workspaces": { body: { data: [RUAHA] } },
      "POST /login": {
        body: {
          data: {
            access_token: "e2e-token-2",
            token_type: "Bearer",
            expires_at: "2099-01-01T00:00:00Z",
            workspace_ids: ["w2"],
          },
        },
      },
    }),
  })
  await page.route("**/v1/me", async (route) => {
    await held
    await route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({ data: { user: BARAKA } }),
    })
  })
  await logIn(page, BARAKA.email)
  await expect(page).toHaveURL("/")
  await expect(page.getByText(RUAHA.display_name)).toBeVisible()
  await expect(page.getByText(AMINA.name)).toHaveCount(0)
  await expect(page.getByText("Ilundo Honey Co.")).toHaveCount(0)
  release()
  await expect(page.getByText(BARAKA.name)).toBeVisible()
})

test.describe("a token the API refuses (expired or revoked)", () => {
  test.use({ allow: { responses: [{ url: "/v1/me", status: 401 }] } })

  test("ends the session and the login page says so", async ({ page }) => {
    await seedSession(page)
    await mockApi(
      page,
      flowApi({
        "GET /me": { status: 401, body: { message: "Unauthenticated." } },
      })
    )
    await readiness(page, 200, { enabled: true, ready: true })
    await page.goto("/")
    await expect(page).toHaveURL("/login")
    await expect(
      page
        .getByRole("status")
        .filter({ hasText: "Your session ended. Log in again to carry on." })
    ).toBeVisible()
    expect(await savedToken(page)).toBeNull()
  })
})
