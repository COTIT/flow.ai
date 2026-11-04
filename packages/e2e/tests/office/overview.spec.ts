import {
  expectNoHorizontalScroll,
  expectNoLayoutShift,
  expectNoMotion,
  expectTapTargets,
} from "../../src/test"
import { expect, expectFits, openMenu, paged, test, type FakeApi } from "./fake-api"

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

const usd = { id: "cur-usd", asset_type: "USD", decimals: 2, name: "US dollar" }

// Opened two days before the run, so it is always "before today".
const staleShift = {
  id: "sh-9",
  register_id: "reg-1",
  opened_by: "tu-1",
  opened_at: new Date(Date.now() - 2 * 86_400_000).toISOString(),
  opening: 5000,
  expected: 5000,
  closed_by: null,
  closed_at: null,
  counted: null,
  difference: null,
}

const sales = {
  from: "2026-11-02T00:00:00+01:00",
  to: "2026-11-09T00:00:00+01:00",
  source: null,
  currencies: [
    { currency: "USD", decimals: 2, orders: 9, gross_total: 123450, refunded_total: 2500 },
  ],
}

const link = {
  id: "link-1",
  partner: { id: "ws-ruaha", name: "RUAHA" },
  direction: "incoming",
  status: "pending",
  fields: ["village"],
  granted_at: "2026-11-02T08:00:00Z",
  accepted_at: null,
  revoked_at: null,
}

const transfer = {
  id: "tr-1",
  direction: "incoming",
  status: "dispatched",
  quantity: "120",
  partner: { id: "ws-ilundo", name: "Ilundo store" },
  link_id: "link-0",
  dispatched_at: "2026-11-03T08:00:00Z",
  from_lot: { id: "lot-x", code: "ILU-H-01", public_code: "7K3M9Q2XRT" },
  received_at: null,
  received_quantity: null,
  to_lot: null,
}

type Totals = typeof totals & { links: number; arriving: number; stale: number }

function overviewApi(api: FakeApi, t: Partial<Totals> = {}) {
  const n = { ...totals, links: 1, arriving: 1, stale: 1, ...t }
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
    // With `to`: the open shifts opened before today. Without: every open shift.
    .on("GET", "/workspace/:ws/till/shifts", ({ query }) =>
      query.get("to") ? paged(n.stale ? [staleShift] : []) : count(n.shifts)
    )
    .on("GET", "/workspace/:ws/till/registers", () =>
      paged([{ id: "reg-1", name: "AFM counter", currency_id: usd.id, warehouse_id: "wh-1" }])
    )
    .on("GET", "/workspace/:ws/currencies", () => paged([usd]))
    .on("GET", "/workspace/:ws/sales-summary", () => ({ data: sales }))
    .on("GET", "/workspace/:ws/supply-links", () => paged(n.links ? [link] : []))
    .on("GET", "/workspace/:ws/supply-transfers", () => paged(n.arriving ? [transfer] : []))
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

    await expect(page.getByRole("region", { name: "Sales this week" })).toBeVisible()
  })
})

test("the counts are read with page_size=1; days and weeks are the browser's own", async ({
  page,
  api,
}) => {
  overviewApi(api)
  const calls: URL[] = []
  await page.route("**/v1/workspace/**", async (route) => {
    calls.push(new URL(route.request().url()))
    await api.handle(route)
  })
  await page.goto("/")
  await expect(page.getByRole("link", { name: /Orders to pick/ })).toContainText("12")
  await expect(page.getByText("Released")).toBeVisible()
  await expect(page.getByText("Supply link from RUAHA")).toBeVisible()
  await expect(page.getByText("$1,234.50")).toBeVisible()
  const sizes = calls.map(
    (u) => `${u.pathname.replace(/^\/v1\/workspace\/[^/]+/, "")}?${u.searchParams}`
  )
  const want = [
    "/orders?status=open&page_size=1",
    "/lots?status=quarantine&page_size=3",
    "/lots?status=released&page_size=1",
    "/lots?status=recalled&page_size=1",
    "/products/stocks/negative?page_size=1",
    "/till/sales?mismatch=true&page_size=1",
    "/till/shifts?open=true&page_size=1",
    "/supply-links?status=pending&direction=incoming&page_size=3",
    "/supply-links?status=pending&direction=incoming&page_size=1",
    "/supply-transfers?direction=incoming&status=dispatched&page_size=3",
  ]
  for (const w of want) expect(sizes, w).toContain(w)

  // Bounds go out as instants; read back in the browser's own zone they are midnights.
  const param = (path: string, key: string, has: string) =>
    calls.find((u) => u.pathname.endsWith(path) && u.searchParams.has(has))?.searchParams.get(key) ?? ""
  const local = (iso: string) =>
    page.evaluate((s) => {
      const d = new Date(s)
      return { day: d.getDay(), minutes: d.getHours() * 60 + d.getMinutes(), date: d.toDateString() }
    }, iso)
  const today = await page.evaluate(() => new Date().toDateString())
  const tomorrow = await page.evaluate(() => {
    const d = new Date()
    d.setDate(d.getDate() + 1)
    return d.toDateString()
  })

  const from = param("/sales-summary", "from", "from")
  const to = param("/sales-summary", "to", "from")
  expect(await local(from)).toMatchObject({ day: 1, minutes: 0 })
  expect(await local(to)).toMatchObject({ day: 1, minutes: 0 })
  const days = (new Date(to).getTime() - new Date(from).getTime()) / 86_400_000
  expect(Math.round(days)).toBe(7)
  expect(new Date(from).getTime()).toBeLessThanOrEqual(Date.now())
  expect(new Date(to).getTime()).toBeGreaterThan(Date.now())

  expect(await local(param("/till/sales", "from", "from"))).toMatchObject({ minutes: 0, date: today })
  expect(await local(param("/till/sales", "to", "from"))).toMatchObject({ minutes: 0, date: tomorrow })
  expect(param("/till/sales", "page_size", "from")).toBe("1")

  expect(await local(param("/till/shifts", "to", "to"))).toMatchObject({ minutes: 0, date: today })
  expect(param("/till/shifts", "open", "to")).toBe("true")
  expect(param("/till/shifts", "page_size", "to")).toBe("3")
})

test("recent orders, needs attention, till, sales this week and batches by status", async ({
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
  // OfficeOverview's other rows: a link to accept, a transfer arriving, a shift left open.
  const linkRow = attention.getByRole("link", { name: /Supply link from RUAHA/ })
  await expect(linkRow).toHaveAttribute("href", "/supply/links")
  await expect(linkRow).toContainText("Waiting for you to accept")
  const transferRow = attention.getByRole("link", { name: /Transfer of 120 kg arriving/ })
  await expect(transferRow).toHaveAttribute("href", "/supply/transfers")
  await expect(transferRow).toContainText("From Ilundo store · receive it")
  const shiftRow = attention.getByRole("link", { name: /Shift open since/ })
  await expect(shiftRow).toHaveAttribute("href", "/till/shifts?open=true")
  await expect(shiftRow).toContainText("AFM counter · count and close")
  await expect(attention.locator("[data-slot=badge]")).toHaveText("6")

  const till = page.getByRole("region", { name: "Till today" })
  await expect(till.getByText("Open shifts")).toBeVisible()
  await expect(till.getByText("7")).toBeVisible()
  await expect(till.getByText("38")).toBeVisible()
  await expect(till.getByRole("link", { name: "Shifts" })).toHaveAttribute(
    "href",
    "/till/shifts?open=true"
  )
  await expect(till).toContainText(
    "Counts come from the till lists; money totals stay on the shift pages."
  )

  // The API's own totals per currency, only formatted; never added up or subtracted here.
  const week = page.getByRole("region", { name: "Sales this week" })
  await expect(week.getByText("Gross")).toBeVisible()
  await expect(week.getByText("$1,234.50")).toBeVisible()
  await expect(week.getByText("Refunded")).toBeVisible()
  await expect(week.getByText("$25.00")).toBeVisible()
  await expect(week.getByText("9 orders")).toBeVisible()
  await expect(week.getByRole("link", { name: "Orders" })).toHaveAttribute("href", "/orders")

  const byStatus = page.getByRole("region", { name: "Batches by status" })
  await expect(byStatus.getByText("18")).toBeVisible()
  await expect(byStatus.getByText("1", { exact: true })).toBeVisible()

  await expectFits(page)
  await openMenu(page)
  await expect(
    page.getByRole("navigation", { name: "Main menu" }).getByRole("link", { name: "Overview" })
  ).toHaveAttribute("aria-current", "page")
})

test("nothing to do and no sales this week say so", async ({ page, api }) => {
  overviewApi(api, { quarantine: 0, negative: 0, mismatch: 0, links: 0, arriving: 0, stale: 0 })
  api.on("GET", "/workspace/:ws/sales-summary", () => ({ data: { ...sales, currencies: [] } }))
  await page.goto("/")
  await expect(
    page.getByRole("region", { name: "Needs attention" }).getByText("Nothing needs you right now.")
  ).toBeVisible()
  await expect(
    page.getByRole("region", { name: "Sales this week" }).getByText("No sales this week")
  ).toBeVisible()
  await expectFits(page)
})

test("the page head goes to receiving and a new batch", async ({ page, api }) => {
  overviewApi(api)
  api.on("GET", "/workspace/:ws/.+", () => paged([]))
  // Not a list: keep its own shape over the catch-all.
  api.on("GET", "/workspace/:ws/sales-summary", () => ({ data: sales }))
  await page.goto("/")
  await page.getByRole("link", { name: "New batch" }).click()
  await expect(page).toHaveURL(/\/lots\?new=1$/)
  await expect(page.getByRole("dialog")).toBeVisible()
  await page.goto("/")
  await expect(page.getByRole("link", { name: "Receive transfer" })).toHaveAttribute("href", "/supply/transfers")
})

test.describe(() => {
  test.use({
    allow: {
      responses: [{ url: "/v1/workspace/w1/till/shifts?open=true&page_size=1", status: 503 }],
    },
  })

  test("a count that fails to load says so and can be retried", async ({ page, api }) => {
    overviewApi(api)
    let fail = true
    api.on("GET", "/workspace/:ws/till/shifts", ({ query }) =>
      query.get("to")
        ? paged([])
        : fail
          ? { status: 503, body: { message: "Shifts are unavailable." } }
          : count(7)
    )
    await page.goto("/")
    const alert = page.getByRole("alert").filter({ hasText: "Shifts are unavailable." })
    await expect(alert).toBeVisible({ timeout: 20_000 })
    fail = false
    await alert.getByRole("button", { name: "Try again" }).click()
    await expect(alert).toHaveCount(0)
    await expect(page.getByRole("region", { name: "Till today" }).getByText("7")).toBeVisible()
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
  await expect(page.getByText("$1,234.50")).toBeVisible()
  await page.waitForTimeout(2500)
  await expect(page.getByRole("link", { name: /Orders to pick/ })).toContainText("12")
  await expectNoLayoutShift(page)
  await expectNoHorizontalScroll(page)
  if ((page.viewportSize()?.width ?? 0) < 768) await expectTapTargets(page)
})
