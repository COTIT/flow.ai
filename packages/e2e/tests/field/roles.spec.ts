import type { Page } from "@playwright/test"

import { mockApi, type MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, role, staffLogin } from "./api"
import { farmerHome } from "./farmer.api"
import { officerHome } from "./officer-records.api"
import { W, crops, total, villages } from "./tower.api"

type Role = "farmer" | "field_officer" | "ops" | "admin"

/** Every surface's home reads, so a test can land anywhere; the guard still fails on anything else. */
const homes: Record<string, MockReply> = {
  ...farmerHome,
  ...officerHome,
  [`GET ${W}/villages`]: villages,
  [`GET ${W}/crops`]: crops,
  [`GET ${W}/pue-requests`]: total(2),
  [`GET ${W}/buyer-demands`]: total(1),
}

const memberships = (roles: Role[]) => roles.map((r) => role(r, r === "ops" || r === "admin" ? null : { id: "v1", name: "Ilundo" }))

/** Signed in with `roles`, every tour seen; opens `path`. */
async function as(page: Page, roles: Role[], path: string) {
  await seedSession(page)
  await page.addInitScript(() =>
    localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["farmer:u1", "officer:u1", "ops:u1"])),
  )
  await mockApi(page, { ...homes, "GET /farm/me": me({ memberships: memberships(roles) }) })
  await page.goto(path)
}

const home: Record<Role, RegExp> = {
  farmer: /\/farm$/,
  field_officer: /\/officer$/,
  ops: /\/ops$/,
  admin: /\/ops$/,
}

for (const r of ["farmer", "field_officer", "ops", "admin"] as const) {
  test(`one ${r} role opens on its own home`, async ({ page }) => {
    await as(page, [r], "/")
    await expect(page).toHaveURL(home[r])
    await expectNoHorizontalScroll(page)
  })
}

test("ops sign in by email and land on the ops home with its queue totals", async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["ops:u1"])))
  await mockApi(page, { ...homes, "POST /login": staffLogin, "GET /farm/me": me({ name: "Juma Ops", memberships: memberships(["ops"]) }) })
  await page.goto("/login")
  await page.getByLabel("Phone number or email").fill("juma@ruaha.test")
  await page.getByLabel("Password").fill("ops-password")
  await page.getByRole("button", { name: "Sign in" }).click()
  await expect(page).toHaveURL(/\/ops$/)
  await expect(page.getByTestId("ops-queue-requests")).toContainText("2")
})

test("two roles pick one on the role picker and land on its home", async ({ page }) => {
  await as(page, ["field_officer", "ops"], "/")
  await expect(page).toHaveURL(/\/select-role$/)
  await expect(page.getByTestId("select-role-field_officer")).toContainText("Field officer")
  await expect(page.getByTestId("select-role-ops")).toContainText("Whole project")
  // The project is named as people know it, not by its slug.
  await expect(page.getByTestId("select-role-ops")).toContainText("Ruaha demo")
  await expect(page.getByTestId("select-role-ops")).not.toContainText("ruaha-demo")
  await expectNoHorizontalScroll(page)

  await page.getByTestId("select-role-ops").click()
  await expect(page).toHaveURL(/\/ops$/)
  await expect(page.getByTestId("ops-queue-requests")).toContainText("2")
})

// The guard is UX (the API is the boundary): a screen outside the role sends the user to their own home.
const outside: [Role, string][] = [
  ["farmer", "/ops/dispatch"],
  ["farmer", "/officer/verify"],
  ["farmer", "/ops/tower"],
  ["field_officer", "/ops/villages"],
  ["field_officer", "/ops/dispatch"],
  ["field_officer", "/farm/passbook"],
  ["ops", "/farm/passbook"],
  ["admin", "/farm"],
]

for (const [r, path] of outside) {
  test(`a ${r} sent to ${path} lands on their own home`, async ({ page }) => {
    await as(page, [r], path)
    await expect(page).toHaveURL(home[r])
  })
}

test("signed out, a deep link goes to sign-in and keeps where it was going", async ({ page }) => {
  await mockApi(page, {})
  await page.goto("/ops/dispatch")
  await expect(page).toHaveURL(/\/login\?redirect=/)
  expect(decodeURIComponent(page.url())).toContain("/ops/dispatch")
})
