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
  const id: string = (await r.json()).data.workspace_ids[0]
  return { name, id, owner: login(email, password) }
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

test("an admin removes a member who left: access ends at once, the list shows them removed", async ({
  page,
  request,
}) => {
  const { id, owner } = await ownWorkspace(request)
  const invitee = `leaver-${unique()}@e2e.test`
  await loginOffice(page, owner)
  await page.goto(`${urls.office}/settings/invites`)
  await page.getByRole("button", { name: "Invite someone" }).click()
  await page.getByRole("dialog").getByLabel("Email").fill(invitee)
  await page.getByRole("dialog").getByRole("button", { name: "Send invite" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: `Invite sent to ${invitee}` })
  ).toBeVisible()
  const code = (await mailTo(invitee)).match(/code=([0-9a-f]{64})/)?.[1]
  expect(code, "invite code").toBeTruthy()
  const joined = await request.post(`${urls.api}/v1/register`, {
    data: {
      first_name: "Peter",
      last_name: "Leaver",
      email: invitee,
      password: `E2e!${unique()}Cc3`,
      invite_code: code,
    },
  })
  expect(joined.status(), "register").toBe(201)
  const token: string = (await joined.json()).data.access_token
  const theirWorkspaces = async () =>
    (
      await (
        await request.get(`${urls.api}/v1/me/workspaces`, {
          headers: { Authorization: `Bearer ${token}` },
        })
      ).json()
    ).data.map((w: { id: string }) => w.id)
  expect(await theirWorkspaces()).toContain(id)

  await page.goto(`${urls.office}/settings/members`)
  const row = page.getByRole("row").filter({ hasText: "Peter Leaver" })
  await expect(row).toContainText("Active")
  await expect(
    page.getByRole("row").filter({ hasText: "Ida Owner" }).getByRole("button", { name: "Remove" })
  ).toHaveCount(0)
  await row.getByRole("button", { name: "Remove" }).click()
  await page.getByRole("alertdialog").getByRole("button", { name: "Remove" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "Peter Leaver no longer has access." })
  ).toBeVisible()
  await expect(row).toContainText("Removed")
  expect(await theirWorkspaces()).not.toContain(id)
})
