import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { flowApi, ILUNDO, mockApi, readiness, RUAHA } from "./api"
import { revealHeader } from "./shell"

test.beforeEach(async ({ page }) => {
  await readiness(page, 200, { enabled: true, ready: true })
})

const saved = (page: import("@playwright/test").Page) =>
  page.evaluate(
    () => JSON.parse(localStorage.getItem("genesy.session")!).state.workspaceId
  )

test("one workspace: its name, no picker", async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi())
  await page.goto("/")
  await expect(page.getByText(ILUNDO.display_name)).toBeVisible()
  await expect(page.getByRole("combobox", { name: "Workspace" })).toHaveCount(0)
})

test("two workspaces: switching changes the shown and saved workspace", async ({
  page,
}) => {
  await seedSession(page, { workspaceIds: ["w1", "w2"] })
  await mockApi(
    page,
    flowApi({ "GET /me/workspaces": { body: { data: [ILUNDO, RUAHA] } } })
  )
  await page.goto("/")
  const picker = page.getByRole("combobox", { name: "Workspace" })
  await expect(picker).toContainText(ILUNDO.display_name)
  await revealHeader(page)
  await picker.click()
  await page
    .getByRole("option", { name: RUAHA.display_name, exact: true })
    .click()
  await expect(picker).toContainText(RUAHA.display_name)
  // The header names who and where (the picker, when shown, also does). Its sr-only status line
  // (P9-3) announces the switch; it is not the visible line.
  await expect(
    page.locator("[data-flow-header] p:not([role=status])", {
      hasText: RUAHA.display_name,
    })
  ).toBeVisible()
  await expect(
    page.locator("[data-flow-header] p[role=status]")
  ).toContainText(RUAHA.display_name)
  expect(await saved(page)).toBe("w2")
  await expectNoHorizontalScroll(page)
})

test("the saved workspace is gone: the member chooses another", async ({
  page,
}) => {
  await seedSession(page, { workspaceIds: ["w1", "w2"] })
  await mockApi(
    page,
    flowApi({ "GET /me/workspaces": { body: { data: [RUAHA] } } })
  )
  await page.goto("/")
  await expect(page.getByText("Choose a workspace")).toBeVisible()
  await page.getByRole("button", { name: RUAHA.display_name }).click()
  await expect(page.getByText("Choose a workspace")).toHaveCount(0)
  // The header names who and where (the picker, when shown, also does).
  await expect(
    page.locator("[data-flow-header] p", { hasText: RUAHA.display_name })
  ).toBeVisible()
  expect(await saved(page)).toBe("w2")
})

test("no workspace at all: says so and how to get one", async ({ page }) => {
  await seedSession(page)
  await mockApi(page, flowApi({ "GET /me/workspaces": { body: { data: [] } } }))
  await page.goto("/")
  await expect(page.getByText("You are not in any workspace yet")).toBeVisible()
  await expect(
    page.getByText("Ask an admin of your account to invite you.")
  ).toBeVisible()
})

test.describe("the workspace list fails", () => {
  test.use({
    allow: { responses: [{ url: "/v1/me/workspaces", status: 500 }] },
  })

  test("shows an error with a retry that recovers", async ({ page }) => {
    await seedSession(page)
    let fail = true
    await mockApi(page, flowApi())
    await page.route("**/v1/me/workspaces", (route) =>
      route.fulfill({
        status: fail ? 500 : 200,
        contentType: "application/json",
        body: JSON.stringify(fail ? { message: "boom" } : { data: [ILUNDO] }),
      })
    )
    await page.goto("/")
    await expect(
      page
        .getByRole("alert")
        .filter({ hasText: "Something went wrong. Try again." })
    ).toBeVisible({ timeout: 15_000 })
    fail = false
    await page.getByRole("button", { name: "Try again" }).click()
    await expect(page.getByText(ILUNDO.display_name)).toBeVisible()
  })
})
