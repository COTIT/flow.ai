import type { Page } from "@playwright/test"

import { expectNoRawValues } from "../../src/test"
import { expect, expectFits, type FakeApi, paged, test } from "./fake-api"

// Stock is kept per place and per batch. These pages must say WHERE a number is and WHICH
// batch it is, and why a place is below zero (a till sold what wasn't recorded there).
const product = {
  id: "prod-wf",
  name: "Wildflower honey",
  reference: "AFM-001",
  price: "8.99",
  status: "active",
  short_description: null,
  description: null,
  store_only: false,
  labels: [],
  total_available_stock: 75,
  total_reserved_stock: 0,
  last_updated_at: "2026-11-06T08:00:00Z",
}
const store = { id: "wh-store", name: "AFM store" }
const stall = { id: "wh-stall", name: "Market stall" }
const depot = { id: "wh-depot", name: "Asheville warehouse" }
const empty = { id: "wh-empty", name: "Moshi depot" }
const lot = { id: "lot-wf", code: "WF-2026-07", expires_on: null }

type Row = {
  id: string
  stock_lvl: string
  reserved: string
  minimal_stock_level: number
  maximum_stock_level: number
  warehouse: { id: string; name: string }
  shelf: null
  bin: null
  lot: typeof lot | null
}

const row = (
  id: string,
  warehouse: Row["warehouse"],
  stock_lvl: string,
  min: number,
  withLot: Row["lot"] = null
): Row => ({
  id,
  stock_lvl,
  reserved: "0.00",
  minimal_stock_level: min,
  maximum_stock_level: 0,
  warehouse,
  shelf: null,
  bin: null,
  lot: withLot,
})

function placesApi(api: FakeApi) {
  const rows = [
    row("st-store", store, "6.00", 10),
    row("st-stall", stall, "-1.00", 0),
    row("st-depot", depot, "36.00", 20),
    row("st-lot", depot, "24.00", 0, lot),
  ]
  api
    .on("GET", "/workspace/:ws/products/:id", () => ({ data: product }))
    .on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => ({ data: rows }))
    .on("GET", "/workspace/:ws/products/:id/stock-movements", () => paged([]))
    .on("GET", "/workspace/:ws/warehouses", () => paged([store, stall, depot, empty]))
    .on("GET", "/workspace/:ws/products/:id/images", () => ({ data: [] }))
    .on("GET", "/workspace/:ws/files", () => paged([]))
    // Like the API: the change lands on the place's row without a batch.
    .on("POST", "/workspace/:ws/products/:id/stocks", ({ body }) => {
      const b = body as { warehouse_id: string; mutation_value: string }
      const r = rows.find((r) => r.warehouse.id === b.warehouse_id && !r.lot)!
      const before = r.stock_lvl
      r.stock_lvl = (Number(r.stock_lvl) + Number(b.mutation_value)).toFixed(2)
      // Like the API: the row's level around the change and where it went.
      return { message: "ok", before, after: r.stock_lvl, warehouse: r.warehouse }
    })
  return rows
}

const panel = (page: Page) => page.getByRole("region", { name: "Stock", exact: true })

test("product stock: each place once, its batches under it, its minimum once, below zero explained", async ({
  page,
  api,
}) => {
  placesApi(api)
  await page.goto("/products/prod-wf")
  const stock = panel(page)
  await expect(stock.getByText("Asheville warehouse", { exact: true })).toHaveCount(1)
  await expect(stock.getByRole("listitem").filter({ hasText: "No batch" })).toContainText("36")
  const batch = stock.getByRole("listitem").filter({ hasText: "Batch WF-2026-07" })
  await expect(batch).toContainText("24")
  await expect(batch.getByRole("link", { name: "Batch WF-2026-07" })).toHaveAttribute(
    "href",
    "/lots/lot-wf"
  )
  // One minimum per place that has one set; none for the stall (0).
  await expect(stock.getByText(/^Minimum/)).toHaveCount(2)
  await expect(stock.getByText("Minimum 20", { exact: true })).toBeVisible()
  await expect(stock.getByText("Minimum 10", { exact: true })).toBeVisible()
  // Whole numbers, no ".00"; the negative one says why.
  await expect(stock.getByText("6", { exact: true })).toBeVisible()
  await expect(stock.getByText("-1", { exact: true })).toBeVisible()
  await expect(stock.getByText(".00")).toHaveCount(0)
  await expect(
    stock.getByText("More was sold here than was recorded. Count it and correct it.")
  ).toBeVisible()
  await expectNoRawValues(page, { allow: ["AFM-001", "WF-2026-07"] })
  await expectFits(page)
})

test("change stock: Correct it opens the dialog at that place; it says what is there and where the change went", async ({
  page,
  api,
}) => {
  placesApi(api)
  await page.goto("/products/prod-wf")
  await panel(page).getByRole("button", { name: "Correct it" }).click()
  const dialog = page.getByRole("dialog")
  const place = dialog.getByLabel("Warehouse")
  await expect(place).toHaveValue("wh-stall")
  await expect(dialog.getByText("Now at Market stall: -1")).toBeVisible()
  // A place with batches: the change goes to the stock without a batch.
  await place.selectOption("wh-depot")
  await expect(dialog.getByText("Now at Asheville warehouse: 36")).toBeVisible()
  await expect(
    dialog.getByText("This changes the stock without a batch.", { exact: false })
  ).toBeVisible()
  await expect(dialog.getByRole("link", { name: "Batch WF-2026-07: 24" })).toHaveAttribute(
    "href",
    "/lots/lot-wf"
  )
  // Edge: a place with nothing yet.
  await place.selectOption("wh-empty")
  await expect(dialog.getByText("Nothing at Moshi depot yet.")).toBeVisible()
  await expect(dialog.getByText("This changes the stock without a batch.", { exact: false })).toHaveCount(0)
  await place.selectOption("wh-store")
  await expect(dialog.getByText("Now at AFM store: 6")).toBeVisible()
  await expectFits(page)
  await dialog.getByLabel("Change (e.g. 10 or -3)").fill("10")
  await dialog.getByRole("button", { name: "Save" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "Added 10 at AFM store: now 16." })
  ).toBeVisible()
  await expect(panel(page).getByText("16", { exact: true })).toBeVisible()
  expect(api.callsTo("POST", "/workspace/:ws/products/:id/stocks")[0]?.body).toEqual({
    warehouse_id: "wh-store",
    mutation_value: "10",
    reason: null,
  })
  // Taking out names the place too.
  await page.getByRole("button", { name: "Change stock" }).click()
  await page.getByRole("dialog").getByLabel("Warehouse").selectOption("wh-stall")
  await page.getByRole("dialog").getByLabel("Change (e.g. 10 or -3)").fill("-3")
  await page.getByRole("dialog").getByRole("button", { name: "Save" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "Took 3 from Market stall: now -4." })
  ).toBeVisible()
  await expectNoRawValues(page, { allow: ["AFM-001", "WF-2026-07"] })
})

test("change stock that moved nothing (a zero change) says nothing moved", async ({
  page,
  api,
}) => {
  placesApi(api)
  api.on("POST", "/workspace/:ws/products/:id/stocks", () => ({ message: "ok" }))
  await page.goto("/products/prod-wf?change=wh-store")
  const dialog = page.getByRole("dialog")
  await dialog.getByLabel("Change (e.g. 10 or -3)").fill("0")
  await dialog.getByRole("button", { name: "Save" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "Nothing moved: the change was 0" })
  ).toBeVisible()
  await expect(page.getByRole("status").filter({ hasText: "Added" })).toHaveCount(0)
})

test("/products/[id]?change= opens Change stock at that place", async ({ page, api }) => {
  placesApi(api)
  await page.goto("/products/prod-wf?change=wh-depot")
  await expect(page.getByRole("dialog").getByLabel("Warehouse")).toHaveValue("wh-depot")
  await expect(page.getByRole("dialog").getByText("Now at Asheville warehouse: 36")).toBeVisible()
})

test.describe(() => {
  test.use({
    allow: { responses: [{ url: "/v1/workspace/w1/products/prod-wf/stocks", status: 422 }] },
  })

  test("change stock refused at a place: the API's words show, the place's number stays", async ({
    page,
    api,
  }) => {
    placesApi(api)
    api.on("POST", "/workspace/:ws/products/:id/stocks", () => ({
      status: 422,
      body: {
        message: "The given data was invalid.",
        errors: { mutation_value: ["Not enough stock here to take that out."] },
      },
    }))
    await page.goto("/products/prod-wf?change=wh-store")
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Change (e.g. 10 or -3)").fill("-50")
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(dialog.getByText("Not enough stock here to take that out.")).toBeVisible()
    await expect(dialog.getByText("Now at AFM store: 6")).toBeVisible()
    await expect(page.getByRole("status").filter({ hasText: "Took" })).toHaveCount(0)
  })
})

test("/stock: batch rows name their batch, numbers read whole, below zero says why and links to the fix", async ({
  page,
  api,
  isMobile,
}) => {
  placesApi(api)
  const listRow = (
    id: string,
    warehouse: Row["warehouse"],
    lvl: string,
    withLot: Row["lot"] = null,
    p: { id: string; name: string } = product
  ) => ({
    id,
    stock_lvl: lvl,
    reserved: "0.00",
    available: lvl,
    minimal_stock_level: 0,
    maximum_stock_level: 0,
    meta: null,
    lot: withLot,
    relationships: { product: p, warehouse },
  })
  const kg = { ...product, id: "prod-kg", name: "Beeswax by weight" }
  api
    .on("GET", "/workspace/:ws/products/stocks", () =>
      paged([
        listRow("s1", store, "16.00"),
        listRow("s2", stall, "-1.00"),
        listRow("s3", depot, "36.00"),
        listRow("s4", depot, "24.00", lot),
        listRow("s5", depot, "2.50", null, kg),
      ])
    )
    .on("GET", "/workspace/:ws/products/stocks/negative", () =>
      paged([listRow("s2", stall, "-1.00")])
    )
  await page.goto("/stock")
  const rows = page.getByRole("row")
  const batchRow = rows.filter({ hasText: "WF-2026-07" })
  await expect(batchRow).toContainText("Asheville warehouse")
  await expect(batchRow.getByRole("link", { name: "Batch WF-2026-07" })).toHaveAttribute(
    "href",
    "/lots/lot-wf"
  )
  await expect(page.getByRole("cell", { name: "16", exact: true }).first()).toBeVisible()
  // A weighed product keeps its decimals.
  await expect(page.getByRole("cell", { name: "2.50", exact: true }).first()).toBeVisible()
  if (!isMobile) await expect(page.getByRole("columnheader", { name: "Held" })).toBeVisible()
  await expect(page.getByText(/\d\.00\b/)).toHaveCount(0)
  const below = rows.filter({ hasText: "Market stall" })
  await expect(below).toContainText("More was sold here than was recorded. Count it and correct it.")
  await expectNoRawValues(page, { allow: ["WF-2026-07"] })
  await expectFits(page)

  await page.getByRole("link", { name: "Below zero" }).click()
  await expect(page).toHaveURL(/\/stock\/negative$/)
  await page.getByRole("link", { name: "Correct it" }).click()
  await expect(page).toHaveURL(/\/products\/prod-wf\?change=wh-stall/)
  await expect(page.getByRole("dialog").getByLabel("Warehouse")).toHaveValue("wh-stall")
  await expect(page.getByRole("dialog").getByText("Now at Market stall: -1")).toBeVisible()
})
