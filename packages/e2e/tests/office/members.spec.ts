import {
  expect,
  expectFits,
  openMenu,
  paged,
  test,
  WS,
  type FakeApi,
} from "./fake-api"

const member = (
  id: string,
  name: string,
  opts: { active?: boolean; owner?: boolean; team?: string } = {}
) => ({
  id,
  name,
  email: `${name.split(" ")[0]!.toLowerCase()}@example.com`,
  user_type: null,
  avatar_path: "",
  initials: name.slice(0, 2).toUpperCase(),
  color: null,
  active: opts.active ?? true,
  permissions: [
    { workspace: WS, permissions: opts.owner ? ["workspace_owner"] : [] },
  ],
  teams: opts.team
    ? [{ id: "t-1", name: opts.team, description: "", permissions: [], meta: null }]
    : [],
  relationships: [],
  created_at: "2026-10-01T08:00:00Z",
})

function membersApi(
  api: FakeApi,
  rows = [
    member("u1", "Romeo Tester", { owner: true, team: "Admin Team" }),
    member("u2", "Peter Mwangi", { team: "Packers" }),
    member("u3", "Amina Juma", { active: false }),
  ]
) {
  const members = [...rows]
  api
    .on("GET", "/workspace/:ws/users", () => paged(members))
    .on("DELETE", "/workspace/:ws/users/:id", ({ params }) => {
      if (params[1] === "u9")
        return {
          status: 409,
          body: { message: "The workspace owner can't be removed." },
        }
      const m = members.find((x) => x.id === params[1])!
      m.active = false
      m.teams = []
      return { status: 204, body: null }
    })
}

test("lists members with team and status; Remove only on others still active", async ({
  page,
  api,
}) => {
  membersApi(api)
  await page.goto("/settings/members")
  await expect(page.getByRole("heading", { level: 1, name: "Members" })).toBeVisible()
  const owner = page.getByRole("row").filter({ hasText: "Romeo Tester" })
  await expect(owner).toContainText("Admin Team")
  await expect(owner).toContainText("Owner")
  await expect(owner.getByRole("button", { name: "Remove" })).toHaveCount(0)
  const peter = page.getByRole("row").filter({ hasText: "Peter Mwangi" })
  await expect(peter).toContainText("Packers")
  await expect(peter).toContainText("Active")
  await expect(peter.getByRole("button", { name: "Remove" })).toBeVisible()
  const amina = page.getByRole("row").filter({ hasText: "Amina Juma" })
  await expect(amina).toContainText("Removed")
  await expect(amina.getByRole("button", { name: "Remove" })).toHaveCount(0)
  await expectFits(page)
})

test("the Settings menu links to members", async ({ page, api }) => {
  membersApi(api)
  api.on("GET", "/workspace/:ws/app-keys", () => paged([]))
  await page.goto("/settings/app-keys")
  await expect(page.getByText("No app keys yet.")).toBeVisible()
  await openMenu(page)
  await page.getByRole("link", { name: "Members" }).click()
  await expect(page).toHaveURL("/settings/members")
})

test("no members to show", async ({ page, api }) => {
  membersApi(api, [])
  await page.goto("/settings/members")
  await expect(page.getByText("No members yet")).toBeVisible()
  await expectFits(page)
})

test("remove: cancelling sends nothing; confirming ends their access", async ({
  page,
  api,
}) => {
  membersApi(api)
  await page.goto("/settings/members")
  const peter = page.getByRole("row").filter({ hasText: "Peter Mwangi" })
  await peter.getByRole("button", { name: "Remove" }).click()
  const dialog = page.getByRole("alertdialog")
  await expect(dialog).toContainText("Remove Peter Mwangi's access?")
  await expect(dialog).toContainText("orders, invoices and history stay")
  await expectFits(page)
  await dialog.getByRole("button", { name: "Cancel" }).click()
  await expect(dialog).toBeHidden()
  expect(api.callsTo("DELETE", "/workspace/:ws/users/:id")).toHaveLength(0)

  await peter.getByRole("button", { name: "Remove" }).click()
  await dialog.getByRole("button", { name: "Remove" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "Peter Mwangi no longer has access." })
  ).toBeVisible()
  await expect(peter).toContainText("Removed")
  await expect(peter.getByRole("button", { name: "Remove" })).toHaveCount(0)
  expect(api.calls.filter((c) => c.method === "DELETE").map((c) => c.path)).toEqual([
    "/workspace/w1/users/u2",
  ])
})

test.describe(() => {
  test.use({
    allow: { responses: [{ url: "/v1/workspace/w1/users/u9", status: 409 }] },
  })

  test("a refusal shows the API's words", async ({ page, api }) => {
    membersApi(api, [
      member("u1", "Romeo Tester", { owner: true }),
      member("u9", "Ida Owner"),
    ])
    await page.goto("/settings/members")
    await page
      .getByRole("row")
      .filter({ hasText: "Ida Owner" })
      .getByRole("button", { name: "Remove" })
      .click()
    await page.getByRole("alertdialog").getByRole("button", { name: "Remove" }).click()
    await expect(
      page.getByRole("alert").filter({ hasText: "The workspace owner can't be removed." })
    ).toBeVisible()
  })
})

test.describe(() => {
  test.use({
    allow: { responses: [{ url: "/v1/workspace/w1/users?page=1", status: 500 }] },
  })

  test("a failed load says so; Try again brings the members", async ({ page, api }) => {
    membersApi(api)
    let fail = true
    api.on("GET", "/workspace/:ws/users", () =>
      fail
        ? { status: 500, body: { message: "The server is busy." } }
        : paged([member("u2", "Peter Mwangi")])
    )
    await page.goto("/settings/members")
    // TanStack retries three times before giving up.
    const alert = page.getByRole("alert").filter({ hasText: "The server is busy." })
    await expect(alert).toBeVisible({ timeout: 20_000 })
    fail = false
    await alert.getByRole("button", { name: "Try again" }).click()
    await expect(page.getByText("Peter Mwangi")).toBeVisible()
    await expect(alert).toHaveCount(0)
  })
})
