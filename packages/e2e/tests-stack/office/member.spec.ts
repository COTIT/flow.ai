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

test("office batches and transfers for a member without product rights: rows only, no refused read, no search", async ({
  page,
  request,
}) => {
  const officer = cred("field (RUAHA)", "field_officer")
  const api = await apiAs(request, officer)
  const me = await api.get<{ user: { permissions: { workspace: string; permissions: string[] }[] } }>("/me")
  const grants = me.user.permissions.find((p) => p.workspace === api.workspace)?.permissions ?? []
  // The seeded officer has no product or search rights, so the gates below are what is tested.
  expect(grants.some((g) => ["product.list", "product.update", "workspace.global-search", "workspace_admin", "workspace_owner"].includes(g))).toBe(false)
  const [first] = await api.get<{ id: string; code: string }[]>(api.ws("/lots?page_size=1"))

  await loginOffice(page, officer)
  await expect(page.getByRole("searchbox", { name: "Quick search" })).toHaveCount(0)
  await expect(page.getByRole("link", { name: "Quick search" })).toHaveCount(0)

  await page.goto(`${urls.office}/lots`)
  await expect(page.getByRole("link", { name: first!.code })).toBeVisible()
  await expect(page.getByRole("button", { name: "New batch" })).toHaveCount(0)
  await expect(page.getByRole("combobox", { name: "Product" })).toHaveCount(0)

  await page.getByRole("link", { name: first!.code }).click()
  await expect(page.getByRole("heading", { name: first!.code })).toBeVisible()
  await page.goto(`${urls.office}/lots/${first!.id}/passport`)
  await expect(page.getByLabel("Title")).toBeVisible()

  await page.goto(`${urls.office}/supply/transfers`)
  await expect(page.getByRole("button", { name: /^Receive batch / })).toHaveCount(0)
  await expectNoRawValues(page)
})
