import type { Page } from "@playwright/test"

import { mockApi } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, test } from "../../src/test"
import { farmLogin, me, role, staffLogin, villages } from "./api"
import { farmerHome } from "./farmer.api"
import { officerHome, tourSeen } from "./officer-records.api"

const TEMP = "Temp-pass-8x"

async function signIn(page: Page, id: string, password: string) {
  await page.getByLabel("Phone number or email").fill(id)
  await page.getByLabel("Password").fill(password)
  await page.getByRole("button", { name: "Sign in" }).click()
}

const officer = {
  ...officerHome,
  "POST /login": staffLogin,
  "GET /farm/me": me(),
  "GET /workspace/w1/farm/villages": villages,
}

const mustChange = me({
  name: "Neema",
  email: null,
  mustChange: true,
  memberships: [role("farmer")],
})

const farmer = {
  ...farmerHome,
  "GET /farm/me": me({
    name: "Neema",
    email: null,
    phone: "+255712345678",
    memberships: [role("farmer")],
  }),
  "GET /workspace/w1/farm/villages": villages,
}

test.describe("sign-in refusals", () => {
  test.describe("email, wrong password", () => {
    test.use({ allow: { responses: [{ url: "/v1/login", status: 401 }] } })

    test("says so and keeps no token", async ({ page }) => {
      await mockApi(page, {
        "POST /login": {
          status: 401,
          body: { message: "Invalid login details" },
        },
      })
      await page.goto("/login")
      await signIn(page, "salima@ruaha.test", "wrong-password")
      await expect(
        page.getByText(
          "That phone number or email and password do not match an account."
        )
      ).toBeVisible()
      await expect(page).toHaveURL(/\/login/)
    })
  })

  test.describe("too many attempts", () => {
    test.use({ allow: { responses: [{ url: "/v1/farm/login", status: 429 }] } })

    // B4: a 429 shows the API's raw "Too Many Attempts." (English only, no advice).
    test.fail(
      "tells the person to wait, in the app's own words",
      async ({ page }) => {
        await mockApi(page, {
          "POST /farm/login": {
            status: 429,
            body: { message: "Too Many Attempts." },
          },
        })
        await page.goto("/login")
        await signIn(page, "0712 345 678", "wrong")
        await expect(page.getByText(/wait|try again later/i)).toBeVisible()
        await expect(page.getByText("Too Many Attempts.")).toHaveCount(0)
      }
    )
  })

  test.describe("no network", () => {
    test.use({ allow: { failures: ["/v1/farm/login"] } })

    test("says the network is down", async ({ page }) => {
      await page.route("**/v1/farm/login", (route) => route.abort())
      await page.goto("/login")
      await signIn(page, "0712 345 678", TEMP)
      await expect(
        page.getByText("No network. Check your connection and try again.")
      ).toBeVisible()
      await expect(page).toHaveURL(/\/login/)
    })
  })
})

test.describe("temporary password", () => {
  test("a farmer who must change it is held at set-password from any deep link", async ({
    page,
  }) => {
    await seedSession(page)
    await mockApi(page, {
      ...farmer,
      "GET /farm/me": me({
        name: "Neema",
        email: null,
        mustChange: true,
        memberships: [role("farmer")],
      }),
    })
    for (const path of ["/farm", "/officer", "/ops"]) {
      await page.goto(path)
      await expect(page).toHaveURL(/\/set-password$/)
    }
  })

  test("two different new passwords are not sent", async ({ page }) => {
    await mockApi(page, {
      "POST /farm/login": farmLogin(true),
      ...farmer,
      "GET /farm/me": mustChange,
    })
    let sent = 0
    page.on("request", (r) => {
      if (r.url().endsWith("/v1/farm/password")) sent++
    })
    await page.goto("/login")
    await signIn(page, "0712 345 678", TEMP)
    await expect(page).toHaveURL(/\/set-password$/)
    await page.getByTestId("set-password-new").fill("my-own-pass-2026")
    await page.getByTestId("set-password-confirm").fill("my-own-pass-2027")
    await page.getByTestId("set-password-submit").click()
    await expect(
      page.getByText("The two passwords are not the same.")
    ).toBeVisible()
    expect(sent).toBe(0)
    await expect(page).toHaveURL(/\/set-password$/)
  })

  test.describe("the API refuses the new password", () => {
    test.use({
      allow: { responses: [{ url: "/v1/farm/password", status: 422 }] },
    })

    test("shows the API's reason and stays", async ({ page }) => {
      await mockApi(page, {
        "POST /farm/login": farmLogin(true),
        ...farmer,
        "GET /farm/me": mustChange,
        "POST /farm/password": {
          status: 422,
          body: { message: "The password has appeared in a data leak." },
        },
      })
      await page.goto("/login")
      await signIn(page, "0712 345 678", TEMP)
      await page.getByTestId("set-password-new").fill("password123")
      await page.getByTestId("set-password-confirm").fill("password123")
      await page.getByTestId("set-password-submit").click()
      await expect(
        page.getByText("The password has appeared in a data leak.")
      ).toBeVisible()
      await expect(page).toHaveURL(/\/set-password$/)
    })
  })
})

test.describe("redirect after sign-in", () => {
  test("a signed-out deep link signs in and comes back", async ({ page }) => {
    await tourSeen(page)
    await mockApi(page, officer)
    await page.goto("/officer")
    await expect(page).toHaveURL(/\/login\?redirect=/)
    await signIn(page, "salima@ruaha.test", "officer-password")
    await expect(page).toHaveURL(/\/officer$/)
    await expect(page.getByTestId("officer-home")).toBeVisible()
  })

  for (const target of [
    "//evil.example.com",
    "https://evil.example.com",
    "javascript:alert(1)",
  ])
    test(`redirect=${target} lands on the own surface`, async ({ page }) => {
      await tourSeen(page)
      await mockApi(page, officer)
      await page.goto(`/login?redirect=${encodeURIComponent(target)}`)
      await signIn(page, "salima@ruaha.test", "officer-password")
      await expect(page).toHaveURL(/^http:\/\/127\.0\.0\.1:\d+\/officer$/)
      await expect(page.getByTestId("officer-home")).toBeVisible()
    })

  test("an encoded double slash stays a path on this site", async ({
    page,
    baseURL,
  }) => {
    await tourSeen(page)
    await mockApi(page, officer)
    await page.goto(
      `/login?redirect=${encodeURIComponent("/%2F%2Fevil.example.com")}`
    )
    await signIn(page, "salima@ruaha.test", "officer-password")
    await expect(page).not.toHaveURL(/\/login/)
    expect(new URL(page.url()).origin).toBe(new URL(baseURL!).origin)
  })
})

test.describe("each role stays on its own surface", () => {
  for (const { who, routes, from, to } of [
    { who: "a farmer", routes: farmer, from: "/officer", to: /\/farm$/ },
    { who: "a farmer", routes: farmer, from: "/ops", to: /\/farm$/ },
    {
      who: "a field officer",
      routes: officer,
      from: "/farm",
      to: /\/officer$/,
    },
    { who: "a field officer", routes: officer, from: "/ops", to: /\/officer$/ },
  ])
    test(`${who} sent to ${from} is turned back`, async ({ page }) => {
      await tourSeen(page)
      await seedSession(page)
      await mockApi(page, routes)
      await page.goto(from)
      await expect(page).toHaveURL(to)
    })

  test("ops may open the officer surface", async ({ page }) => {
    // Ops keep the ops nav (and so the ops tour) on officer screens: mark that tour seen, or it
    // starts and takes them to /ops before the check below.
    await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["ops:u1"])))
    await seedSession(page)
    await mockApi(page, {
      ...officer,
      "GET /farm/me": me({ memberships: [role("ops", null)] }),
    })
    await page.goto("/officer")
    await expect(page).toHaveURL(/\/officer$/)
    await expect(page.getByTestId("officer-home")).toBeVisible()
  })
})

test("with two roles, picking one opens its surface", async ({ page }) => {
  await tourSeen(page)
  await mockApi(page, {
    ...officer,
    "GET /farm/me": me({
      memberships: [role("field_officer"), role("farmer")],
    }),
    ...farmerHome,
  })
  await page.goto("/login")
  await signIn(page, "salima@ruaha.test", "officer-password")
  await expect(page).toHaveURL(/\/select-role$/)
  await page.getByRole("button", { name: /Field officer/ }).click()
  await expect(page).toHaveURL(/\/officer$/)
})

test.describe("a token the API stops accepting", () => {
  test.use({
    allow: {
      responses: [
        "village_id=v1&page_size=1",
        "village_id=v1&page_size=1&verification=unverified,pending",
      ].map((q) => ({
        url: `/v1/workspace/w1/farm/persons?${q}`,
        status: 401,
      })),
    },
  })

  // B5: a mid-session 401 leaves the officer on a dead screen; the app never
  // returns to sign-in or says the session ended.
  test.fail("sends the person back to sign in", async ({ page }) => {
    await tourSeen(page)
    await seedSession(page)
    await mockApi(page, {
      ...officer,
      "GET /workspace/w1/farm/persons": {
        status: 401,
        body: { message: "You are not authorized to perform this action." },
      },
    })
    await page.goto("/officer")
    await expect(page).toHaveURL(/\/login/)
  })
})
