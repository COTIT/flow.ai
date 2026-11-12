// P3 row actions + scene steps over a SIMULATED API and a fake agent: each scene row offers its
// next step. Writes only ever go to Genesy as a request (the normal review card follows); links
// open that exact record in Office in a new tab.
import type { Page } from "@playwright/test"

import { expectNoHorizontalScroll } from "../../src/test"
import { rowAction } from "./lists"
import { HONEY, TEA, menuItem, moreMenu, sc, sceneWorld } from "./scenes-api"
import { expect, member, test } from "./storefront-api"
import { stockReview } from "./task"
import discoveryEn from "../../../../apps/flow/messages/en/discovery.json" with { type: "json" }
import { OFFICE, fakeAgent } from "./voice"

const O1 = "01950000-0000-7000-8000-0000000000b1"
const O2 = "01950000-0000-7000-8000-0000000000b2"
const I1 = "01950000-0000-7000-8000-0000000000c1"
const order = (id: string, number: string) => ({
  id,
  number,
  status: "open",
  created_at: "2026-11-10T09:00:00Z",
  total: 125050,
  currency: "EUR",
  decimals: 2,
  customer: null,
})
const item = (ref_id: string, label: string) => ({
  ref_id,
  label,
  available: null,
  minimum: null,
  shortfall: null,
  open_orders: null,
  acknowledged: false,
  money: null,
  since: null,
})
const group = (kind: string, items: unknown[], actions: string[]) => ({
  kind,
  count: items.length,
  open_count: items.length,
  acknowledged_count: 0,
  snoozed_count: 0,
  snoozed: false,
  headline_ref: null,
  impact: { open_orders: null },
  actions,
  items,
})

/** Every request that would change stock (none may leave Flow: Genesy prepares the review). */
function stockWrites(page: Page) {
  const writes: string[] = []
  page.on("request", (r) => {
    if (r.method() !== "GET" && /stock/.test(new URL(r.url()).pathname))
      writes.push(`${r.method()} ${r.url()}`)
  })
  return writes
}

const restock = (page: Page, id: string) =>
  page.locator(`[data-row="${id}"]`).getByRole("button", { name: /^Restock / })

test.beforeEach(({ api }) => void api)

test("Restock asks Genesy with the product and its shortfall; no amount decided, nothing written", async ({
  page,
  api,
}) => {
  sceneWorld(api)
  const writes = stockWrites(page)
  const sent: string[] = []
  await fakeAgent(page, (text) => sent.push(text))
  await page.goto("/scenes/stock")
  await restock(page, HONEY).click()
  await expect.poll(() => sent.length).toBe(1)
  // The product and how far below its minimum, from the API; how many arrived is Genesy's question.
  expect(sent[0]).toBe("Restock Raw Honey 500g. It is 6 below its minimum.")
  expect(sent[0]).not.toMatch(/\bto \d|minimum of|set it/i)
  // Selected for Genesy (id only), like tapping the name.
  await expect(page.locator(`[data-row="${HONEY}"] [data-record]`)).toHaveAttribute(
    "aria-pressed",
    "true"
  )
  expect(writes).toEqual([])
  await expectNoHorizontalScroll(page)
})

test("Restock is hidden without the right to set stock levels", async ({ page, api }) => {
  sceneWorld(api)
  api.on("GET", "/me", () => ({ data: { user: member } }))
  await page.goto("/scenes/stock")
  await expect(page.locator(`[data-row="${HONEY}"]`)).toBeVisible()
  await expect(page.getByRole("button", { name: /^Restock / })).toHaveCount(0)
})

test("while a review waits, no row can start another request", async ({ page, api }) => {
  sceneWorld(api)
  await page.route("**/api/voice/tools", async (route) => {
    const sent = route.request().postDataJSON() as { command?: string; operationId?: string }
    if (sent.command === "execute" && sent.operationId === "stock_set_levels")
      return route.fulfill({ json: stockReview(20) })
    return route.fallback()
  })
  const agent = await fakeAgent(page, () =>
    agent.call("stock_set_levels", {
      path: { product: HONEY },
      body: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 20 }] },
    })
  )
  await page.goto("/scenes/stock")
  await restock(page, HONEY).click()
  // The review waits (the dock is held): no row offers another request until it is decided.
  await expect(page.locator("[data-held]").first()).toBeAttached()
  await expect(restock(page, TEA)).toHaveCount(0)
})

test("Order Focus: Open in Office opens that order in a new tab; only record ids pass", async ({
  page,
  api,
}) => {
  sceneWorld(api)
  api.on("GET", "/workspace/w1/orders", () => ({
    data: [order(O1, "ORD-0042"), { ...order(O2, "ORD-0043"), id: "not-a-record-id" }],
    meta: { current_page: 1, last_page: 1, per_page: 10, total: 2 },
  }))
  await page.goto("/scenes/orders")
  const open = page
    .locator(`[data-row="${O1}"]`)
    .getByRole("link", { name: /^Open in Office: ORD-0042/ })
  await expect(open).toBeVisible()
  await expect(open).toHaveAttribute("href", `${OFFICE}/orders/${O1}`)
  await expect(open).toHaveAttribute("target", "_blank")
  await expect(open).toHaveAttribute("rel", /noopener/)
  await expect(
    page.locator('[data-row="not-a-record-id"]').getByRole("link")
  ).toHaveCount(0)
  await expectNoHorizontalScroll(page)
})

test("Flow noticed: each item's next step is its button; acknowledge and snooze wait under Later", async ({
  page,
  api,
}) => {
  const world = sceneWorld(api)
  const marks: unknown[] = []
  api
    .on("GET", "/workspace/w1/attention/priorities", () => ({
      data: {
        generated_at: "2026-11-11T06:00:00Z",
        restricted: [],
        groups: [
          group(
            "stock.below_minimum",
            [
              {
                ...item(HONEY, "Raw Honey 500g"),
                available: "14",
                minimum: "20",
                shortfall: "6",
              },
            ],
            ["review", "acknowledge", "snooze"]
          ),
          group("orders.open", [item(O1, "ORD-0042")], ["review", "acknowledge", "snooze"]),
          group("invoices.overdue", [item(I1, "Neema Shop")], ["acknowledge", "snooze"]),
        ],
      },
    }))
    .on("PUT", "/workspace/w1/attention/marks", ({ body }) => {
      marks.push(body)
      return { data: { id: "m1", ...(body as object) } }
    })
  void world
  await page.goto("/scenes/noticed")
  const honey = page.locator(`[data-item="${HONEY}"]`)
  await expect(honey.getByRole("button", { name: /^Restock / })).toBeVisible()
  await expect(
    page.locator(`[data-item="${O1}"]`).getByRole("link", { name: /^Open in Office/ })
  ).toHaveAttribute("href", `${OFFICE}/orders/${O1}`)
  await expect(
    page.locator(`[data-item="${I1}"]`).getByRole("link", { name: /^Open in Office/ })
  ).toHaveAttribute("href", `${OFFICE}/invoices/${I1}`)
  // Acknowledge is no longer the row's button: it waits with Snooze.
  await expect(
    honey.getByRole("button", { name: sc.noticed.acknowledge, exact: true })
  ).toHaveCount(0)
  await rowAction(page, honey, "acknowledge")
  await expect.poll(() => marks).toContainEqual({
    kind: "stock.below_minimum",
    ref_id: HONEY,
    state: "acknowledged",
  })
  await expectNoHorizontalScroll(page)
})

test("Sales: Compare with last week shows both weeks from the API's own answer", async ({
  page,
  api,
}) => {
  sceneWorld(api)
  await page.goto("/scenes/sales")
  await moreMenu(page).click()
  const compare = menuItem(page, sc.compare.button)
  await expect(compare).toHaveAttribute("aria-checked", "false")
  await expect(page.locator("[data-compare]")).toHaveCount(0)
  await compare.click()
  // An on/off choice keeps the menu open: its state shows right there.
  await expect(compare).toHaveAttribute("aria-checked", "true")
  await page.keyboard.press("Escape")
  const panel = page.locator("[data-compare]")
  await expect(panel.getByRole("heading")).toHaveText(sc.compare.title.this_week)
  const sales = panel.locator('[data-item$="-gross"]')
  // current 12,840.00, base 10,700.00, gross_difference 2,140.00, gross_change_bp 2000.
  await expect(sales).toContainText("€12,840.00")
  await expect(sales).toContainText("€10,700.00")
  await expect(sales).toContainText("+€2,140.00")
  await expect(sales).toContainText("+20%")
  await expect(panel.locator('[data-item$="-orders"]')).toContainText(sc.compare.orders)
  await expectNoHorizontalScroll(page)
  await moreMenu(page).click()
  await compare.click()
  await expect(page.locator("[data-compare]")).toHaveCount(0)
})

test("Sales compare: without the API's percentage none is shown", async ({ page, api }) => {
  sceneWorld(api)
  const sums = (gross: number) => ({
    gross_total: gross,
    net_total: gross,
    orders: 3,
    refunded_total: 0,
    vat_total: 0,
  })
  api.on("GET", "/workspace/w1/sales/compare", () => ({
    data: {
      time_zone: "Africa/Dar_es_Salaam",
      period: { from: "2026-11-09T00:00:00Z", to: "2026-11-12T00:00:00Z" },
      compare_to_period: { from: "2026-11-02T00:00:00Z", to: "2026-11-05T00:00:00Z" },
      currencies: [
        {
          currency: "EUR",
          decimals: 2,
          current: sums(50000),
          base: sums(0),
          gross_difference: 50000,
          net_difference: 50000,
          orders_difference: 3,
          gross_change_bp: null,
          net_change_bp: null,
        },
      ],
    },
  }))
  await page.goto("/scenes/sales")
  await moreMenu(page).click()
  await menuItem(page, sc.compare.button).click()
  const panel = page.locator("[data-compare]")
  await expect(panel).toContainText("+€500.00")
  await expect(panel).not.toContainText("%")
})

test("Sales compare is not offered when the comparison is not readable", async ({ page, api }) => {
  sceneWorld(api, { role: "ops" })
  await page.goto("/scenes/sales")
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
  await moreMenu(page).click()
  await expect(menuItem(page, sc.compare.button)).toHaveCount(0)
})

test("empty My Routines: one big Set up a weekly report asks Genesy", async ({ page, api }) => {
  sceneWorld(api)
  api.on("GET", "/workspace/w1/scheduled-reports", () => ({ data: [] }))
  const sent: string[] = []
  await fakeAgent(page, (text) => sent.push(text))
  await page.goto("/scenes/routines")
  await expect(page.getByText(sc.routines.emptyTitle)).toBeVisible()
  const setUp = page.getByRole("button", { name: sc.routines.setUp })
  expect((await setUp.boundingBox())!.height).toBeGreaterThanOrEqual(56)
  await setUp.click()
  await expect
    .poll(() => sent)
    .toEqual([discoveryEn.discovery.capabilities.schedule_weekly_report.prompt])
  await expectNoHorizontalScroll(page)
})
