import type { Page } from "@playwright/test"

import { mockApi } from "../../src/api-mock"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { farmLogin, me, role, staffLogin, villages } from "./api"
import { farmerHome } from "./farmer.api"
import { officerHome, tourSeen } from "./officer-records.api"

const TEMP = "Temp-pass-8x"

async function signIn(page: Page, id: string, password: string) {
  await page.goto("/login")
  await page.getByLabel("Phone number or email").fill(id)
  await page.getByLabel("Password").fill(password)
  await page.getByRole("button", { name: "Sign in" }).click()
}

/** Every key and value in local and session storage, as one string. */
const allStorage = (page: Page) =>
  page.evaluate(() =>
    [localStorage, sessionStorage]
      .flatMap((s) => Object.keys(s).map((k) => `${k}=${s.getItem(k)}`))
      .join("\n")
  )

test("an officer signs in by email and lands on the officer surface", async ({ page }) => {
  await mockApi(page, {
    ...officerHome,
    "POST /login": staffLogin,
    "GET /farm/me": me(),
    "GET /workspace/w1/farm/villages": villages,
  })
  await signIn(page, "salima@ruaha.test", "officer-password")

  await expect(page).toHaveURL(/\/officer$/)
  await expect(page.getByTestId("officer-home")).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test("the account's stored language is applied after sign-in", async ({ page }) => {
  await mockApi(page, {
    ...officerHome,
    "POST /login": staffLogin,
    "GET /farm/me": me({ locale: "sw" }),
    "GET /workspace/w1/farm/villages": villages,
  })
  await signIn(page, "salima@ruaha.test", "officer-password")

  await expect(page.locator("html")).toHaveAttribute("lang", "sw")
  await expect(page.getByText("Vijiji vyako").first()).toBeVisible()
})

test("a farmer signs in by phone, must change the temporary password, and it never touches storage", async ({ page }) => {
  await mockApi(page, {
    "POST /farm/login": farmLogin(true),
    "GET /farm/me": me({ name: "Neema", email: null, phone: "+255712345678", mustChange: true, memberships: [role("farmer")] }),
    "GET /workspace/w1/farm/villages": villages,
    ...farmerHome,
  })
  const login = page.waitForRequest("**/v1/farm/login")
  await signIn(page, "0712 345 678", TEMP)
  expect((await login).postDataJSON()).toEqual({ phone: "0712 345 678", password: TEMP })

  await expect(page).toHaveURL(/\/set-password$/)
  // The temporary password is still in memory, so it is not asked for again.
  await expect(page.getByTestId("set-password-current")).toHaveCount(0)
  expect(await allStorage(page)).not.toContain(TEMP)
  await expectNoHorizontalScroll(page)

  // After the change the account no longer has the flag.
  await mockApi(page, {
    "POST /farm/password": { status: 204 },
    "GET /farm/me": me({ name: "Neema", email: null, phone: "+255712345678", memberships: [role("farmer")] }),
    "GET /workspace/w1/farm/villages": villages,
    ...farmerHome,
  })
  const change = page.waitForRequest("**/v1/farm/password")
  await page.getByTestId("set-password-new").fill("my-own-pass-2026")
  await page.getByTestId("set-password-confirm").fill("my-own-pass-2026")
  await page.getByTestId("set-password-submit").click()
  expect((await change).postDataJSON()).toEqual({ current_password: TEMP, password: "my-own-pass-2026" })

  await expect(page).toHaveURL(/\/farm$/)
  expect(await allStorage(page)).not.toContain(TEMP)
})

test("after a reload the farmer types the temporary password", async ({ page }) => {
  await mockApi(page, {
    "POST /farm/login": farmLogin(true),
    "GET /farm/me": me({ name: "Neema", email: null, mustChange: true, memberships: [role("farmer")] }),
    ...farmerHome,
  })
  await signIn(page, "0712 345 678", TEMP)
  await expect(page).toHaveURL(/\/set-password$/)
  await page.reload()
  await expect(page.getByTestId("set-password-current")).toBeVisible()
})

test.describe("wrong password", () => {
  test.use({ allow: { responses: [{ url: "/v1/farm/login", status: 401 }] } })

  test("says so and stays on the sign-in page", async ({ page }) => {
    await mockApi(page, { "POST /farm/login": { status: 401, body: { message: "Unauthenticated." } } })
    await signIn(page, "0712 345 678", "wrong")
    await expect(page.getByText("That phone number or email and password do not match an account.")).toBeVisible()
    await expect(page).toHaveURL(/\/login/)
  })
})

test("someone with no farm role is told so", async ({ page }) => {
  await mockApi(page, {
    ...officerHome,
    "POST /login": staffLogin,
    "GET /farm/me": me({ memberships: [] }),
  })
  await signIn(page, "member@ruaha.test", "member-password")
  await expect(page).toHaveURL(/\/no-access$/)
  await expect(page.getByText("No access yet")).toBeVisible()
})

test("two roles open the role picker", async ({ page }) => {
  await mockApi(page, {
    ...officerHome,
    "POST /login": staffLogin,
    "GET /farm/me": me({ memberships: [role("field_officer"), role("ops", null)] }),
    "GET /workspace/w1/farm/villages": villages,
  })
  await signIn(page, "salima@ruaha.test", "officer-password")
  await expect(page).toHaveURL(/\/select-role$/)
  await expect(page.getByText("Choose a role")).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test("signing out ends the token and forgets it", async ({ page }) => {
  await mockApi(page, {
    ...officerHome,
    "POST /login": staffLogin,
    "GET /farm/me": me(),
    "GET /workspace/w1/farm/villages": villages,
    "POST /logout": { status: 204 },
  })
  // The first-visit tour would cover the header; it is tested in its own spec.
  await tourSeen(page)
  await signIn(page, "salima@ruaha.test", "officer-password")
  await expect(page).toHaveURL(/\/officer$/)

  const logout = page.waitForRequest("**/v1/logout")
  // At 375 px sign-out sits in the user menu; on desktop it is in the header.
  const menu = page.getByTestId("user-menu-trigger")
  if (await menu.isVisible()) await menu.click()
  await page.getByTestId("sign-out").filter({ visible: true }).click()
  await logout
  await expect(page).toHaveURL(/\/login/)
  expect(await allStorage(page)).not.toContain("e2e-token")
})
