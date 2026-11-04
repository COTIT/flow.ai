import type { Page } from "@playwright/test"

import { seedSession } from "../../src/session"
import { test as guarded } from "../../src/test"
import { expect, fakeApi, openMenu, paged, test } from "./fake-api"

const product = (id: string, name: string) => ({
  id,
  name,
  product_nr: id,
  reference: null,
  price_incl: "9.00",
  status: "active",
})

const tokenFor = (email: string) => `token-${email.split("@")[0]}`

/** The login reply for `email`: its own token, one workspace (w1). */
const loggedIn = (email: string) => ({
  data: {
    access_token: tokenFor(email),
    token_type: "Bearer",
    expires_at: "2099-01-01T00:00:00Z",
    workspace_ids: ["w1"],
  },
})

async function logIn(page: Page, email: string) {
  await page.getByLabel("Email").fill(email)
  await page.getByLabel("Password").fill("e2e-password-only")
  await page.getByRole("button", { name: "Log in" }).click()
}

/** The token the office keeps in localStorage, or null once signed out. */
const storedToken = (page: Page) =>
  page.evaluate(
    () =>
      JSON.parse(localStorage.getItem("genesy.session") ?? "{}").state?.token ??
      null
  )

test.describe("login refusals", () => {
  for (const { status, message, shown } of [
    {
      status: 429,
      message: "Too Many Attempts.",
      shown: "Too many attempts. Wait a few minutes and try again.",
    },
    {
      status: 422,
      message: "The email field must be a valid email address.",
      shown: "Check the email and password fields.",
    },
  ])
    test.describe(`${status}`, () => {
      test.use({ allow: { responses: [{ url: "/v1/login", status }] } })

      guarded(`says "${shown}" and stays signed out`, async ({ page }) => {
        const api = await fakeApi(page)
        api.on("POST", "/login", () => ({ status, body: { message } }))
        await page.goto("/login")
        await logIn(page, "e2e@genesy.test")
        await expect(
          page.getByRole("alert").filter({ hasText: shown })
        ).toBeVisible()
        await expect(page).toHaveURL("/login")
        expect(await storedToken(page)).toBeNull()
      })
    })

  test.describe("403", () => {
    test.use({ allow: { responses: [{ url: "/v1/login", status: 403 }] } })

    // B2: the login page maps only 401/422/429 and drops the API's words for a 403.
    guarded.fail(
      "an account with no workspace is told so, not a generic error",
      async ({ page }) => {
        const api = await fakeApi(page)
        api.on("POST", "/login", () => ({
          status: 403,
          body: { message: "No workspace found associated with your account" },
        }))
        await page.goto("/login")
        await logIn(page, "e2e@genesy.test")
        await expect(
          page.getByRole("alert").filter({ hasText: /no workspace/i })
        ).toBeVisible()
      }
    )
  })
})

test("a signed-in visit to the login page goes on to its next page", async ({
  page,
  api,
}) => {
  api.on("GET", "/workspace/:ws/products", () => paged([]))
  await page.goto("/login?next=/products")
  await expect(page).toHaveURL("/products")
  await expect(page.getByText("No products yet.")).toBeVisible()
})

test.describe("logout", () => {
  for (const { name, fail } of [
    { name: "the API answers 500", fail: false },
    { name: "the network drops", fail: true },
  ])
    guarded.describe(`when ${name}`, () => {
      guarded.use({
        allow: fail
          ? { failures: ["/v1/logout"] }
          : { responses: [{ url: "/v1/logout", status: 500 }] },
      })

      guarded("still signs out and forgets the token", async ({ page }) => {
        await seedSession(page)
        const api = await fakeApi(page)
        api.on("GET", "/workspace/:ws/products", () => paged([]))
        await page.route("**/v1/logout", (route) =>
          fail
            ? route.abort()
            : route.fulfill({ status: 500, body: '{"message":"boom"}' })
        )
        await page.goto("/products")
        await openMenu(page)
        await page.getByRole("button", { name: "Log out" }).click()
        await expect(page).toHaveURL("/login")
        expect(await storedToken(page)).toBeNull()
      })
    })
})

test.describe("a refused token", () => {
  test.use({
    allow: { responses: [{ url: "/v1/workspace/w1/products", status: 401 }] },
  })

  test("is forgotten, and the ended-session note goes once the user logs in again", async ({
    page,
    api,
  }) => {
    let refused = true
    api
      .on("GET", "/workspace/:ws/products", () =>
        refused
          ? { status: 401, body: { message: "Unauthenticated." } }
          : paged([])
      )
      .on("POST", "/login", ({ body }) => {
        refused = false
        return loggedIn((body as { email: string }).email)
      })
    await page.goto("/products")
    await expect(page).toHaveURL(/\/login$/)
    const ended = page
      .getByRole("status")
      .filter({ hasText: "Your session ended." })
    await expect(ended).toBeVisible()
    expect(await storedToken(page)).toBeNull()

    await page.evaluate(() =>
      history.replaceState(null, "", "/login?next=/products")
    )
    await logIn(page, "e2e@genesy.test")
    await expect(page.getByText("No products yet.")).toBeVisible()
    await openMenu(page)
    await page.getByRole("button", { name: "Log out" }).click()
    await expect(page).toHaveURL("/login")
    await expect(ended).toHaveCount(0)
  })

  // B3: an expired session goes to /login with no way back to the page it was on.
  test.fail(
    "keeps the page it was on as the login's next",
    async ({ page, api }) => {
      api.on("GET", "/workspace/:ws/products", () => ({
        status: 401,
        body: { message: "Unauthenticated." },
      }))
      await page.goto("/products")
      await expect(page).toHaveURL("/login?next=%2Fproducts")
    }
  )
})

guarded(
  "the next user never sees the last user's rows, even while loading",
  async ({ page }) => {
    const api = await fakeApi(page)
    let user = ""
    api.on("POST", "/login", ({ body }) => {
      user = (body as { email: string }).email
      return loggedIn(user)
    })
    api.on("GET", "/workspace/:ws/products", () =>
      paged(user.startsWith("ana") ? [product("p1", "Ana's private jar")] : [])
    )
    await page.goto("/login?next=/products")
    await logIn(page, "ana@genesy.test")
    await expect(
      page.getByRole("cell", { name: "Ana's private jar" })
    ).toBeVisible()
    await openMenu(page)
    await page.getByRole("button", { name: "Log out" }).click()
    await expect(page).toHaveURL("/login")

    // Ben's list is held until checked, so a cached copy of Ana's would show meanwhile.
    let release!: () => void
    const held = new Promise<void>((r) => (release = r))
    await page.route("**/v1/workspace/w1/products*", async (route) => {
      await held
      await api.handle(route)
    })
    // No reload: a reload would drop any in-memory cache this test is about.
    await page.evaluate(() =>
      history.replaceState(null, "", "/login?next=/products")
    )
    await logIn(page, "ben@genesy.test")
    await expect(page.getByRole("heading", { name: "Products" })).toBeVisible()
    await expect(page.getByText("Ana's private jar")).toHaveCount(0)
    release()
    await expect(page.getByText("No products yet.")).toBeVisible()
    await expect(page.getByText("Ana's private jar")).toHaveCount(0)
    const ben = api.callsTo("GET", "/workspace/:ws/products").at(-1)
    expect(ben?.headers.authorization).toBe(
      `Bearer ${tokenFor("ben@genesy.test")}`
    )
  }
)
