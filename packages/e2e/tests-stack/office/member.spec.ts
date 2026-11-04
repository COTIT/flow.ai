import { expectNoRawValues } from "../../src/test"
import { apiAs, cred, expect, loginOffice, test, urls } from "../../src/stack"

// A workspace member without office rights (a RUAHA field officer): the overview asks only for
// what they may read, so the guard sees no 403 at all, and the tiles they may not see are gone.
test("office overview for a member without office rights: no refused request, only their tiles", async ({ page }) => {
  await loginOffice(page, cred("field (RUAHA)", "field_officer"))
  await expect(page.getByText("Batches by status")).toBeVisible()
  await expect(page.getByText("Needs attention")).toBeVisible()
  for (const title of ["Orders to pick", "Recent orders", "Sales this week", "Till today", "Products below zero"])
    await expect(page.getByText(title, { exact: true }), title).toHaveCount(0)
  await expectNoRawValues(page)
})

test("office overview for the owner still shows every tile", async ({ page }) => {
  await loginOffice(page, cred("office (AFM)", "owner"))
  for (const title of ["Orders to pick", "Recent orders", "Sales this week", "Till today", "Batches by status"])
    await expect(page.getByText(title, { exact: true }), title).toBeVisible()
})

// The pages a member may not use: menu entry gone, and the address says so without a refused read.
const gated: [path: string, right: string, menu?: string][] = [
  ["/products", "product.list", "Products"],
  ["/orders", "order.list", "Orders"],
  ["/invoices", "invoice.list", "Invoices"],
  ["/till/registers", "cashregister.list", "Till"],
  ["/settings/app-keys", "workspace_admin", "App keys"],
  ["/settings/invites", "invite.list", "Invites"],
]

test("office menu and pages for a member without office rights: only what they may use", async ({
  page,
  request,
}) => {
  const officer = cred("field (RUAHA)", "field_officer")
  const api = await apiAs(request, officer)
  const me = await api.get<{ user: { permissions: { workspace: string; permissions: string[] }[] } }>("/me")
  const grants = me.user.permissions.find((p) => p.workspace === api.workspace)?.permissions ?? []
  const may = (right: string) =>
    [right, "workspace_admin", "workspace_owner"].some((g) => grants.includes(g))

  await loginOffice(page, officer)
  if ((page.viewportSize()?.width ?? 0) < 768) await page.getByRole("button", { name: "Open menu" }).click()
  const nav = page.getByRole("navigation", { name: "Main menu" })
  await expect(nav.getByRole("link", { name: "Batches" })).toBeVisible()
  for (const [, right, menu] of gated)
    if (menu) await expect(nav.getByRole("link", { name: menu, exact: true }), menu).toHaveCount(may(right) ? 1 : 0)
  await page.keyboard.press("Escape")

  const refused = gated.filter(([, right]) => !may(right))
  // The seeded officer has no office rights, so this proves something.
  expect(refused.length).toBeGreaterThan(0)
  for (const [path] of refused) {
    await page.goto(`${urls.office}${path}`)
    await expect(page.getByText("You don't have access to this"), path).toBeVisible()
    await expect(page.getByRole("link", { name: "Go to the overview" })).toBeVisible()
  }
})
