// Keyboard focus when Flow itself sends a person to the login page (logout, expiry, or browser
// Back into the signed-in app after logging out): it lands on the login heading, never <body>.
// SIMULATED API (mockApi).
import type { Page } from "@playwright/test"

import { seedSession } from "../../src/session"
import { expect, test } from "../../src/test"
import { flowApi, mockApi } from "./api"
import { sc } from "./scenes-api"
import { revealHeader } from "./shell"

const heading = (page: Page) => page.getByRole("heading", { level: 1, name: "Log in" })
const onBody = (page: Page) => page.evaluate(() => document.activeElement === document.body)

test.beforeEach(async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi())
})

test("log out by keyboard, then browser Back and Forward: focus stays on the login page, never <body>", async ({
  page,
}) => {
  await page.goto("/")
  // Somewhere in the app first, so Back after logging out leads into the signed-in app.
  await page.getByRole("region", { name: sc.yourFlow }).getByRole("link").first().click()
  await expect(page).toHaveURL(/\/scenes\//)
  const logout = (await revealHeader(page)).getByRole("button", { name: "Log out" })
  await logout.focus()
  await page.keyboard.press("Enter")
  await expect(page).toHaveURL(/\/login$/)
  await expect(heading(page)).toBeFocused()

  // Back into the signed-in app: it sends the logged-out person straight back here.
  await page.goBack()
  await expect(page).toHaveURL(/\/login$/)
  await expect(heading(page)).toBeVisible()
  await page.waitForTimeout(200)
  expect(await onBody(page), "focus lost to <body> after Back").toBe(false)
  await expect(heading(page)).toBeFocused()

  await page.goForward().catch(() => {}) // nothing ahead after the replace is fine
  await expect(page).toHaveURL(/\/login$/)
  await page.waitForTimeout(200)
  expect(await onBody(page), "focus lost to <body> after Forward").toBe(false)

  // The next Tab goes into the form, not back to the top.
  await page.keyboard.press("Tab")
  await expect(page.getByLabel("Email")).toBeFocused()
})

test("a full load of the login page keeps the browser's own start (no focus jump)", async ({ page }) => {
  await page.addInitScript(() => localStorage.removeItem("genesy.session"))
  await page.goto("/login")
  await expect(heading(page)).toBeVisible()
  await page.waitForTimeout(200)
  expect(await onBody(page)).toBe(true)
})
