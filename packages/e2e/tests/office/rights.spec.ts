import { expect, type FakeApi, me, openMenu, paged, test, WS } from "./fake-api"

// Each office page, the right the API checks for the list it reads, and its menu entry.
const routes: { path: string; read: string; right: string; menu?: string }[] = [
  { path: "/products", read: "/workspace/:ws/products", right: "product.list", menu: "Products" },
  { path: "/stock", read: "/workspace/:ws/products/stocks", right: "stock.list", menu: "Stock" },
  { path: "/orders", read: "/workspace/:ws/orders", right: "order.list", menu: "Orders" },
  { path: "/invoices", read: "/workspace/:ws/invoices", right: "invoice.list", menu: "Invoices" },
  { path: "/supply/links", read: "/workspace/:ws/supply-links", right: "workspace_admin", menu: "Supply" },
  { path: "/till/registers", read: "/workspace/:ws/till/registers", right: "cashregister.list", menu: "Till" },
  { path: "/settings/app-keys", read: "/workspace/:ws/app-keys", right: "workspace_admin", menu: "App keys" },
  { path: "/settings/webhooks", read: "/workspace/:ws/webhook-endpoints", right: "workspace_admin", menu: "Webhooks" },
  { path: "/settings/invites", read: "/workspace/:ws/invites", right: "invite.list", menu: "Invites" },
  { path: "/search?q=honey", read: "/workspace/:ws/quick-search", right: "workspace.global-search" },
]

function asMember(api: FakeApi, grants: string[]) {
  api.on("GET", "/me", () => ({
    data: { user: { ...me, permissions: [{ workspace: WS, permissions: grants }] } },
  }))
  // Like the API: whatever the member may not read answers 403 (the guard fails the test on any).
  for (const r of routes)
    if (!grants.includes(r.right))
      api.on("GET", r.read, () => ({ status: 403, body: { message: "This action is unauthorized." } }))
  // What every member may read: batches, transfers and currencies.
  api
    .on("GET", "/workspace/:ws/lots", () => paged([]))
    .on("GET", "/workspace/:ws/supply-transfers", () => paged([]))
    .on("GET", "/workspace/:ws/currencies", () => paged([]))
}

for (const grants of [[], ["order.list", "invoice.list"]])
  test(`a member with ${grants.length ? grants.join(" + ") : "no grants"} sees only the menu entries they may use`, async ({
    page,
    api,
  }) => {
    asMember(api, grants)
    api.on("GET", "/workspace/:ws/sales-summary", () => ({ data: { from: "", to: "", source: null, currencies: [] } }))
    await page.goto("/")
    await expect(page.getByText("Batches by status")).toBeVisible()
    await openMenu(page)
    const nav = page.getByRole("navigation", { name: "Main menu" })
    await expect(nav.getByRole("link", { name: "Overview" })).toBeVisible()
    await expect(nav.getByRole("link", { name: "Batches" })).toBeVisible()
    for (const r of routes.filter((x) => x.menu))
      await expect(nav.getByRole("link", { name: r.menu!, exact: true }), r.menu).toHaveCount(
        grants.includes(r.right) ? 1 : 0
      )
  })

for (const r of routes)
  test(`a member without ${r.right} who opens ${r.path} is told they have no access; nothing is read`, async ({
    page,
    api,
  }) => {
    asMember(api, [])
    await page.goto(r.path)
    await expect(page.getByText("You don't have access to this")).toBeVisible()
    await expect(page.getByText("Ask an admin of this account to give you access.")).toBeVisible()
    await expect(page.getByRole("link", { name: "Go to the overview" })).toHaveAttribute("href", "/")
    expect(api.callsTo("GET", r.read), r.read).toHaveLength(0)
  })

test("a member with the right opens the page as usual", async ({ page, api }) => {
  asMember(api, ["order.list"])
  api.on("GET", "/workspace/:ws/orders", () => paged([]))
  await page.goto("/orders")
  await expect(page.getByText("No orders yet.")).toBeVisible()
  await expect(page.getByText("You don't have access to this")).toHaveCount(0)
})

test("the owner still has every menu entry", async ({ page, api }) => {
  api.on("GET", "/workspace/:ws/supply-transfers", () => paged([]))
  await page.goto("/supply/transfers")
  await openMenu(page)
  const nav = page.getByRole("navigation", { name: "Main menu" })
  for (const r of routes.filter((x) => x.menu))
    await expect(nav.getByRole("link", { name: r.menu!, exact: true }), r.menu).toBeVisible()
})
