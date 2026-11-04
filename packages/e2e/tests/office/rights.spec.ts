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

const lot = {
  id: "lot-1",
  product_id: "prod-honey",
  code: "ILU-2026-01",
  public_code: "7K3M9Q2XRT",
  status: "released",
  produced_on: "2026-09-01",
  best_before: null,
  expires_on: null,
  origin: "Ilundo, Tanzania",
  created_at: "2026-09-02T08:00:00Z",
  updated_at: "2026-09-02T08:00:00Z",
}
const transfer = {
  id: "tr-1",
  link_id: "link-1",
  direction: "incoming",
  partner: { id: "ws-ruaha", name: "RUAHA Project" },
  from_lot: { code: "ILU-H-2026-01", public_code: "7K3M9Q2XRT" },
  quantity: "250.00",
  status: "dispatched",
  dispatched_at: "2026-10-31T08:00:00Z",
  to_lot: null,
  received_quantity: null,
  received_at: null,
}

/** Batches and transfers for a member; product, warehouse and file reads answer 403 unless granted. */
function supplyApi(api: FakeApi, grants: string[]) {
  asMember(api, grants)
  const refuse = (path: string, right: string) => {
    if (!grants.includes(right))
      api.on("GET", path, () => ({ status: 403, body: { message: "This action is unauthorized." } }))
  }
  api
    .on("GET", "/workspace/:ws/lots", () => paged([lot]))
    .on("GET", "/workspace/:ws/lots/:lot", () => ({ data: lot }))
    .on("GET", "/workspace/:ws/lots/:lot/events", () => paged([]))
    .on("GET", "/workspace/:ws/lots/:lot/passport", () => ({
      data: { draft: { title: "Ilundo honey" }, published: null, published_at: null, version: 0 },
    }))
    .on("GET", "/workspace/:ws/supply-transfers", () => paged([transfer]))
    .on("GET", "/workspace/:ws/products/:id", () => ({ data: { id: "prod-honey", name: "AFM Honey 500g" } }))
    .on("GET", "/workspace/:ws/files", () => paged([]))
    .on("GET", "/workspace/:ws/warehouses", () => paged([{ id: "wh-1", name: "Dar es Salaam" }]))
  if (grants.includes("product.list"))
    api.on("GET", "/workspace/:ws/products", () => paged([{ id: "prod-honey", name: "AFM Honey 500g" }]))
  refuse("/workspace/:ws/products/:id", "product.view")
  refuse("/workspace/:ws/files", "file.list")
  refuse("/workspace/:ws/warehouses", "warehouse.list")
}

test("/lots for a member without product rights: the rows, no product read, no product filter, no New batch", async ({
  page,
  api,
}) => {
  supplyApi(api, [])
  await page.goto("/lots")
  await expect(page.getByRole("link", { name: "ILU-2026-01" })).toBeVisible()
  await expect(page.getByRole("combobox", { name: "Product" })).toHaveCount(0)
  await expect(page.getByRole("button", { name: "New batch" })).toHaveCount(0)
  expect(api.callsTo("GET", "/workspace/:ws/products")).toHaveLength(0)
})

test("/lots for a member who may read products but not change them: names and filter, no New batch", async ({
  page,
  api,
}) => {
  supplyApi(api, ["product.list"])
  await page.goto("/lots")
  await expect(page.getByRole("cell", { name: "AFM Honey 500g" })).toBeVisible()
  await expect(page.getByRole("combobox", { name: "Product" })).toBeVisible()
  await expect(page.getByRole("button", { name: "New batch" })).toHaveCount(0)
})

test("/lots/[id] and its passport for a member without product or file rights: no refused read", async ({
  page,
  api,
}) => {
  supplyApi(api, [])
  await page.goto("/lots/lot-1")
  await expect(page.getByRole("heading", { name: "ILU-2026-01" })).toBeVisible()
  await page.goto("/lots/lot-1/passport")
  await expect(page.getByLabel("Title")).toHaveValue("Ilundo honey")
  expect(api.callsTo("GET", "/workspace/:ws/products/:id")).toHaveLength(0)
  expect(api.callsTo("GET", "/workspace/:ws/files")).toHaveLength(0)
})

for (const [grants, offered] of [
  [[], false],
  [["product.list", "warehouse.list"], false],
  [["product.update", "product.list", "warehouse.list"], true],
] as const)
  test(`/supply/transfers for a member with ${grants.length ? grants.join(" + ") : "no grants"}: Receive ${offered ? "offered" : "hidden"}`, async ({
    page,
    api,
  }) => {
    supplyApi(api, [...grants])
    await page.goto("/supply/transfers")
    await expect(page.getByText("RUAHA Project")).toBeVisible()
    const receive = page.getByRole("button", { name: "Receive batch ILU-H-2026-01" })
    await expect(receive).toHaveCount(offered ? 1 : 0)
    if (offered) {
      await receive.click()
      await expect(page.getByRole("dialog").getByLabel("New batch code")).toBeVisible()
    }
  })

test("the quick search is there only for a member who may search", async ({ page, api }) => {
  supplyApi(api, [])
  await page.goto("/lots")
  await expect(page.getByRole("link", { name: "ILU-2026-01" })).toBeVisible()
  await expect(page.getByRole("searchbox", { name: "Quick search" })).toHaveCount(0)
  await expect(page.getByRole("link", { name: "Quick search" })).toHaveCount(0)
})

test("the owner keeps the quick search", async ({ page, api }) => {
  api.on("GET", "/workspace/:ws/supply-transfers", () => paged([]))
  await page.goto("/supply/transfers")
  const phone = (page.viewportSize()?.width ?? 0) < 768
  await expect(
    phone ? page.getByRole("link", { name: "Quick search" }) : page.getByRole("searchbox", { name: "Quick search" })
  ).toBeVisible()
})
