import {
  expectNoHorizontalScroll,
  expectNoLayoutShift,
  expectNoMotion,
  expectTapTargets,
} from "../../src/test"
import { expect, openMenu, paged, test, type FakeApi } from "./fake-api"

// The overview (OfficeOverview): every number is a list's `meta.total`, read with page_size=1,
// so the browser adds nothing up.
const totals = {
  openOrders: 12,
  quarantine: 3,
  released: 18,
  recalled: 1,
  negative: 4,
  mismatch: 2,
  shifts: 7,
  sales: 38,
}

/** A page_size=1 reply whose total is `total` (the row itself is never shown). */
const count = (total: number, row: object = { id: "x" }) => ({
  ...paged(total ? [row] : [], 1, 1),
  meta: { ...paged([], 1, 1).meta, total, last_page: Math.max(1, total) },
})

const lot = {
  id: "lot-1",
  code: "CF-2026-03",
  status: "quarantine",
  product_id: "p1",
  public_code: "7K3M9QX2WD",
  created_at: "2026-10-28T09:00:00Z",
}

const order = {
  id: "ord-1",
  number: "SO-1042",
  status: "awaiting_payment",
  source: "storefront",
  currency: "USD",
  decimals: 2,
  customer: { name: "Dana Whitfield", email: "dana@example.com" },
  total: 4850,
  created_at: "2026-11-03T09:12:00Z",
}

function overviewApi(api: FakeApi, t: Partial<typeof totals> = {}) {
  const n = { ...totals, ...t }
  api
    .on("GET", "/workspace/:ws/orders", ({ query }) =>
      query.get("status") === "open"
        ? count(n.openOrders)
        : paged([order], 1, Number(query.get("page_size") ?? 50))
    )
    .on("GET", "/workspace/:ws/lots", ({ query }) => {
      const status = query.get("status")
      if (status === "quarantine")
        return { ...paged(n.quarantine ? [lot] : [], 1, 3), meta: { ...paged([]).meta, total: n.quarantine } }
      return count(status === "released" ? n.released : n.recalled)
    })
    .on("GET", "/workspace/:ws/products/stocks/negative", () => count(n.negative))
    .on("GET", "/workspace/:ws/till/sales", ({ query }) =>
      count(query.get("mismatch") === "true" ? n.mismatch : n.sales)
    )
    .on("GET", "/workspace/:ws/till/shifts", () => count(n.shifts))
}

test.describe("reduced motion", () => {
  test.use({ reducedMotion: "reduce" })

  test("home is the overview: every count is a list total, nothing animates", async ({
    page,
    api,
  }) => {
    overviewApi(api)
    await page.goto("/")
    await expect(page.getByRole("heading", { level: 1, name: "Overview" })).toBeVisible()
    const kpi = (name: string) => page.getByRole("link", { name: new RegExp(name) })
    await expect(kpi("Orders to pick")).toContainText("12")
    await expect(kpi("Batches in quarantine")).toContainText("3")
    await expect(kpi("Products below zero")).toContainText("4")
    await expect(kpi("Till sales at other prices")).toContainText("2")
    await expect(kpi("Orders to pick")).toHaveAttribute("href", "/orders?status=open")
    await expect(kpi("Batches in quarantine")).toHaveAttribute("href", "/lots?status=quarantine")
    await expect(kpi("Products below zero")).toHaveAttribute("href", "/stock/negative")
    await expect(kpi("Till sales at other prices")).toHaveAttribute("href", "/till/sales?mismatch=true")
    await expectNoMotion(page)

    await expect(page.getByText("Sales this week")).toHaveCount(0)
  })
})

test("the counts are read with page_size=1", async ({ page, api }) => {
  overviewApi(api)
  const sizes: string[] = []
  await page.route("**/v1/workspace/**", async (route) => {
    const url = new URL(route.request().url())
    sizes.push(`${url.pathname.replace(/^\/v1\/workspace\/[^/]+/, "")}?${url.searchParams}`)
    await api.handle(route)
  })
  await page.goto("/")
  await expect(page.getByRole("link", { name: /Orders to pick/ })).toContainText("12")
  await expect(page.getByText("Released")).toBeVisible()
  const want = [
    "/orders?status=open&page_size=1",
    "/lots?status=quarantine&page_size=3",
    "/lots?status=released&page_size=1",
    "/lots?status=recalled&page_size=1",
    "/products/stocks/negative?page_size=1",
    "/till/sales?mismatch=true&page_size=1",
    "/till/sales?page_size=1",
    "/till/shifts?page_size=1",
  ]
  for (const w of want) expect(sizes, w).toContain(w)
})

test("recent orders, needs attention, till and batches by status", async ({
  page,
  api,
}) => {
  overviewApi(api)
  await page.goto("/")
  const recent = page.getByRole("region", { name: "Recent orders" })
  await expect(recent.getByRole("link", { name: "SO-1042" })).toHaveAttribute("href", "/orders/ord-1")
  await expect(recent.getByText("Dana Whitfield")).toBeVisible()
  await expect(recent.getByRole("link", { name: /All orders/ })).toHaveAttribute("href", "/orders")

  const attention = page.getByRole("region", { name: "Needs attention" })
  await expect(attention.getByRole("link", { name: /CF-2026-03/ })).toHaveAttribute("href", "/lots/lot-1")
  await expect(attention.getByText("4 products below zero")).toBeVisible()
  await expect(attention.getByText("2 till sales at other prices")).toBeVisible()

  const till = page.getByRole("region", { name: "Till" })
  await expect(till.getByText("7")).toBeVisible()
  await expect(till.getByText("38")).toBeVisible()

  const byStatus = page.getByRole("region", { name: "Batches by status" })
  await expect(byStatus.getByText("18")).toBeVisible()
  await expect(byStatus.getByText("1", { exact: true })).toBeVisible()

  await openMenu(page)
  await expect(
    page.getByRole("navigation", { name: "Main menu" }).getByRole("link", { name: "Overview" })
  ).toHaveAttribute("aria-current", "page")
})

test("nothing to do says so", async ({ page, api }) => {
  overviewApi(api, { quarantine: 0, negative: 0, mismatch: 0 })
  await page.goto("/")
  await expect(
    page.getByRole("region", { name: "Needs attention" }).getByText("Nothing needs you right now.")
  ).toBeVisible()
})

test("the page head goes to receiving and a new batch", async ({ page, api }) => {
  overviewApi(api)
  api.on("GET", "/workspace/:ws/.+", () => paged([]))
  await page.goto("/")
  await page.getByRole("link", { name: "New batch" }).click()
  await expect(page).toHaveURL(/\/lots\?new=1$/)
  await expect(page.getByRole("dialog")).toBeVisible()
  await page.goto("/")
  await expect(page.getByRole("link", { name: "Receive transfer" })).toHaveAttribute("href", "/supply/transfers")
})

test.describe(() => {
  test.use({
    allow: { responses: [{ url: "/v1/workspace/w1/till/shifts?page_size=1", status: 503 }] },
  })

  test("a count that fails to load says so and can be retried", async ({ page, api }) => {
    overviewApi(api)
    let fail = true
    api.on("GET", "/workspace/:ws/till/shifts", () =>
      fail ? { status: 503, body: { message: "Shifts are unavailable." } } : count(7)
    )
    await page.goto("/")
    const alert = page.getByRole("alert").filter({ hasText: "Shifts are unavailable." })
    await expect(alert).toBeVisible({ timeout: 20_000 })
    fail = false
    await alert.getByRole("button", { name: "Try again" }).click()
    await expect(alert).toHaveCount(0)
    await expect(page.getByRole("region", { name: "Till" }).getByText("7")).toBeVisible()
  })
})

test("the numbers count up once, then sit still; the page does not shift", async ({ page, api }) => {
  overviewApi(api)
  // Slow answers, so the skeletons really paint before the data replaces them.
  await page.route("**/v1/workspace/**", async (route) => {
    await new Promise((r) => setTimeout(r, 400))
    await api.handle(route)
  })
  await page.goto("/")
  await expect(page.locator("[data-motion]").first()).toBeAttached()
  await expect(page.getByRole("link", { name: /Orders to pick/ })).toContainText("12", { timeout: 10_000 })
  await page.waitForTimeout(2500)
  await expect(page.getByRole("link", { name: /Orders to pick/ })).toContainText("12")
  await expectNoLayoutShift(page)
  await expectNoHorizontalScroll(page)
  if ((page.viewportSize()?.width ?? 0) < 768) await expectTapTargets(page)
})
