import type { APIRequestContext } from "@playwright/test"

import {
  expect,
  loginOffice,
  mailTo,
  test,
  unique,
  urls,
} from "../../src/stack"

/** A login of the test's own, in the shape `loginOffice` takes. */
const login = (email: string, password: string) => ({
  app: "office (e2e)",
  role: "owner",
  login: email,
  secret: password,
  path: "/login",
})

/**
 * A workspace of the test's own (signup is on in the stack), its owner verified through the
 * link the verification mail carries: only a verified owner may invite (data rule 1, no seeded user).
 */
async function ownWorkspace(request: APIRequestContext) {
  const name = `e2e-inv-${unique()}`
  const email = `owner-${name}@e2e.test`
  const password = `E2e!${unique()}Aa1`
  const r = await request.post(`${urls.api}/v1/create-workspace`, {
    data: {
      first_name: "Ida",
      last_name: "Owner",
      email,
      password,
      workspace_name: name,
    },
  })
  expect(r.status(), "signup").toBe(201)
  const link = (await mailTo(email)).match(
    /https?:\/\/\S+\/v1\/verify-email\/\S+/
  )?.[0]
  expect(link, "verification link").toBeTruthy()
  const verified = await request.get(link!, { maxRedirects: 0 })
  expect(verified.headers().location).toContain("email_verified=1")
  return { name, owner: login(email, password) }
}

test("an office invite reaches a new person, who joins and sees the account", async ({
  page,
  browser,
  request,
}) => {
  const { name, owner } = await ownWorkspace(request)
  const invitee = `member-${unique()}@e2e.test`

  await loginOffice(page, owner)
  await page.goto(`${urls.office}/settings/invites`)
  await page.getByRole("button", { name: "Invite someone" }).click()
  const dialog = page.getByRole("dialog")
  await dialog.getByLabel("Email").fill(invitee)
  await dialog.getByRole("button", { name: "Send invite" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: `Invite sent to ${invitee}` })
  ).toBeVisible()
  await expect(
    page.getByRole("row").filter({ hasText: invitee })
  ).toContainText("Pending")

  // The invitee opens the mailed link on another device (a fresh browser context).
  const link = (await mailTo(invitee)).match(
    /https?:\/\/\S+\/accept-invitation\?code=\S+/
  )?.[0]
  expect(link, "invite link").toBeTruthy()
  expect(new URL(link!).origin).toBe(urls.office)
  const other = await browser.newContext()
  const them = await other.newPage()
  await them.goto(link!)
  await expect(
    them.getByRole("heading", { name: `Join ${name}` })
  ).toBeVisible()
  await expect(them.getByLabel("Email")).toHaveValue(invitee)
  await them.getByLabel("First name").fill("Neema")
  await them.getByLabel("Last name").fill("Member")
  await them.getByLabel("Password").fill(`E2e!${unique()}Bb2`)
  await them.getByRole("button", { name: "Create account" }).click()
  await expect(them).toHaveURL(`${urls.office}/`)
  await expect(
    them.getByRole("heading", { level: 1, name: "Overview" })
  ).toBeVisible()
  const menu = them.getByRole("button", { name: "Open menu" })
  if (await menu.isVisible()) await menu.click()
  await expect(them.getByRole("combobox", { name: "Workspace" })).toHaveText(
    new RegExp(name)
  )

  // The same link again is spent.
  await them.context().clearCookies()
  await them.evaluate(() => localStorage.clear())
  await them.goto(link!)
  await expect(
    them.getByRole("alert").or(them.getByRole("status")).first()
  ).toBeVisible()
  await expect(
    them.getByRole("button", { name: "Create account" })
  ).toHaveCount(0)
  await other.close()

  // The owner sees the invite accepted.
  await page.reload()
  await expect(
    page.getByRole("row").filter({ hasText: invitee })
  ).toContainText("Accepted")
})
