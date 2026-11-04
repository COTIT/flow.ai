import {
  expect,
  expectFits,
  openMenu,
  paged,
  test,
  type FakeApi,
} from "./fake-api"

const team = (id: string, name: string) => ({
  id,
  name,
  description: "",
  permissions: [],
  created_at: "2026-10-01T08:00:00Z",
  last_updated_at: "2026-10-01T08:00:00Z",
  meta: null,
  relationships: [],
})

const invite = (
  id: string,
  email: string,
  status: string,
  teamRow: ReturnType<typeof team> | null
) => ({
  invite_id: id,
  email,
  invite_status: status,
  invited_by: "romeo@genesy.test",
  created_at: "2026-11-01T08:00:00Z",
  expired_at: "2026-11-08T08:00:00Z",
  last_updated_at: "2026-11-01T08:00:00Z",
  meta: null,
  relationships: { team: teamRow },
})

function invitesApi(api: FakeApi, rows = [
  invite("inv-1", "amina@example.com", "pending", team("t-1", "Packers")),
  invite("inv-2", "juma@example.com", "accepted", null),
]) {
  const invites = [...rows]
  api
    .on("GET", "/workspace/:ws/invites", () => paged(invites))
    .on("GET", "/workspace/:ws/users/teams", () =>
      paged([team("t-1", "Packers"), team("t-2", "Office")])
    )
    .on("POST", "/workspace/:ws/invites", ({ body }) => {
      const b = body as { email: string; team_id: string | null }
      if (b.email === "taken@example.com")
        return {
          status: 422,
          body: {
            message: "The given data was invalid.",
            errors: { email: ["This person already has an invite."] },
          },
        }
      if (b.email === "unverified@example.com")
        return {
          status: 403,
          body: { message: "Verify your email address first." },
        }
      invites.push(
        invite("inv-3", b.email, "pending", b.team_id ? team(b.team_id, "Office") : null)
      )
      return { status: 201, body: { data: { invite_id: "inv-3" } } }
    })
    .on("DELETE", "/workspace/:ws/invites/:id", ({ params }) => {
      invites.splice(
        invites.findIndex((i) => i.invite_id === params[1]),
        1
      )
      return { status: 200, body: { message: "Invite withdrawn." } }
    })
}

test("lists invites with team, status and dates", async ({ page, api }) => {
  invitesApi(api)
  await page.goto("/settings/invites")
  await expect(
    page.getByRole("heading", { level: 1, name: "Invites" })
  ).toBeVisible()
  const pending = page.getByRole("row").filter({ hasText: "amina@example.com" })
  await expect(pending).toContainText("Packers")
  await expect(pending).toContainText("Pending")
  await expect(
    pending.getByRole("button", { name: "Withdraw" })
  ).toBeVisible()
  const accepted = page.getByRole("row").filter({ hasText: "juma@example.com" })
  await expect(accepted).toContainText("—")
  await expect(accepted).toContainText("Accepted")
  await expect(accepted.getByRole("button", { name: "Withdraw" })).toHaveCount(0)
  await expectFits(page)
})

test("the Settings menu links to invites", async ({ page, api }) => {
  invitesApi(api)
  api.on("GET", "/workspace/:ws/app-keys", () => paged([]))
  await page.goto("/settings/app-keys")
  await expect(page.getByText("No app keys yet.")).toBeVisible()
  await openMenu(page)
  await page.getByRole("link", { name: "Invites" }).click()
  await expect(page).toHaveURL("/settings/invites")
})

test("no invites yet", async ({ page, api }) => {
  invitesApi(api, [])
  await page.goto("/settings/invites")
  await expect(page.getByText("No invites yet")).toBeVisible()
  await expectFits(page)
})

test("send an invite with a team: body exact, list refreshed", async ({
  page,
  api,
}) => {
  invitesApi(api)
  await page.goto("/settings/invites")
  await page.getByRole("button", { name: "Invite someone" }).click()
  const dialog = page.getByRole("dialog")
  await dialog.getByLabel("Email").fill("neema@example.com")
  await dialog.getByLabel("Team (role)").selectOption({ label: "Office" })
  await expectFits(page)
  await dialog.getByRole("button", { name: "Send invite" }).click()
  await expect(dialog).toBeHidden()
  expect(api.callsTo("POST", "/workspace/:ws/invites")[0]?.body).toEqual({
    email: "neema@example.com",
    team_id: "t-2",
  })
  await expect(
    page.getByRole("status").filter({ hasText: "Invite sent to neema@example.com" })
  ).toBeVisible()
  await expect(
    page.getByRole("row").filter({ hasText: "neema@example.com" })
  ).toBeVisible()
})

test("no team sends team_id null", async ({ page, api }) => {
  invitesApi(api)
  await page.goto("/settings/invites")
  await page.getByRole("button", { name: "Invite someone" }).click()
  const dialog = page.getByRole("dialog")
  await expect(dialog.getByLabel("Team (role)")).toHaveValue("")
  await dialog.getByLabel("Email").fill("solo@example.com")
  await dialog.getByRole("button", { name: "Send invite" }).click()
  await expect(dialog).toBeHidden()
  expect(api.callsTo("POST", "/workspace/:ws/invites")[0]?.body).toEqual({
    email: "solo@example.com",
    team_id: null,
  })
})

test.describe(() => {
  test.use({
    allow: { responses: [{ url: "/v1/workspace/w1/invites", status: 403 }] },
  })

  test("an unverified sender sees the API's words", async ({ page, api }) => {
    invitesApi(api)
    await page.goto("/settings/invites")
    await page.getByRole("button", { name: "Invite someone" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Email").fill("unverified@example.com")
    await dialog.getByRole("button", { name: "Send invite" }).click()
    await expect(
      dialog.getByRole("alert").filter({ hasText: "Verify your email address first." })
    ).toBeVisible()
  })
})

test.describe(() => {
  test.use({
    allow: { responses: [{ url: "/v1/workspace/w1/invites", status: 422 }] },
  })

  test("a 422 shows next to the email", async ({ page, api }) => {
    invitesApi(api)
    await page.goto("/settings/invites")
    await page.getByRole("button", { name: "Invite someone" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Email").fill("taken@example.com")
    await dialog.getByRole("button", { name: "Send invite" }).click()
    await expect(dialog.getByText("This person already has an invite.")).toBeVisible()
    await expect(dialog.getByLabel("Email")).toHaveAttribute("aria-invalid", "true")
  })
})

test("withdraw: cancelling sends nothing; confirming removes it", async ({
  page,
  api,
}) => {
  invitesApi(api)
  await page.goto("/settings/invites")
  await page.getByRole("button", { name: "Withdraw" }).click()
  await expect(page.getByRole("alertdialog")).toContainText(
    "Withdraw the invite for amina@example.com?"
  )
  await page.getByRole("button", { name: "Cancel" }).click()
  await expect(page.getByRole("alertdialog")).toBeHidden()
  expect(api.callsTo("DELETE", "/workspace/:ws/invites/:id")).toHaveLength(0)

  await page.getByRole("button", { name: "Withdraw" }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Withdraw" })
    .click()
  await expect(
    page.getByRole("status").filter({ hasText: "Invite withdrawn." })
  ).toBeVisible()
  await expect(page.getByText("amina@example.com")).toHaveCount(0)
  expect(api.calls.filter((c) => c.method === "DELETE").map((c) => c.path)).toEqual([
    "/workspace/w1/invites/inv-1",
  ])
})
