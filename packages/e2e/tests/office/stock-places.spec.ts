import type { Page } from "@playwright/test"

import { expectNoRawValues } from "../../src/test"
import { expect, expectFits, type FakeApi, me, paged, test, WS } from "./fake-api"

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

const generalStock = {
  id: "wh-general",
  name: "General stock",
  slug: "general-stock",
}
const warehousePath = "/workspace/:ws/warehouses"

test("empty Change stock creates General stock and saves stock there", async ({
  page,
  api,
}) => {
  placesApi(api)
  const warehouseRows: (typeof generalStock)[] = []
  api
    .on("GET", warehousePath, () => paged(warehouseRows))
    .on("POST", warehousePath, () => {
      warehouseRows.push(generalStock)
      return { status: 201, body: { data: { warehouse_id: generalStock.id } } }
    })
    .on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => ({
      data: [],
    }))
    .on("POST", "/workspace/:ws/products/:id/stocks", () => ({
      message: "ok",
      before: "0",
      after: "8",
      warehouse: generalStock,
    }))

  await page.goto("/products/prod-wf?change=")
  const dialog = page.getByRole("dialog")
  await expect(
    dialog.getByRole("link", { name: "Manage warehouses" })
  ).toHaveAttribute("href", "/stock/warehouses")
  await expect(
    dialog.getByRole("button", { name: "Save", exact: true })
  ).toBeDisabled()
  await dialog.getByRole("button", { name: "Create General stock" }).click()
  await expect(dialog.getByLabel("Warehouse", { exact: true })).toHaveValue(
    generalStock.id
  )
  await expect(
    dialog.getByRole("button", { name: "Create General stock" })
  ).toHaveCount(0)
  await dialog.getByLabel("Change (e.g. 10 or -3)").fill("8")
  await dialog.getByRole("button", { name: "Save", exact: true }).click()
  await expect(dialog).toBeHidden()
  await expect(page.getByText("Added 8", { exact: false })).toContainText(
    generalStock.name
  )
  expect(api.callsTo("POST", warehousePath).map((call) => call.body)).toEqual([
    { name: "General stock" },
  ])
  expect(
    api
      .callsTo("POST", "/workspace/:ws/products/:id/stocks")
      .map((call) => call.body)
  ).toEqual([
    { warehouse_id: generalStock.id, mutation_value: "8", reason: null },
  ])
})

test("Change stock resolves other warehouse pages and requires a replacement for a stale selection", async ({
  page,
  api,
}) => {
  placesApi(api)
  api.on("GET", warehousePath, ({ query }) =>
    paged(
      [store, depot],
      Number(query.get("page") ?? 1),
      query.get("roots_only") === "false" ? 1 : 50
    )
  )

  await page.goto("/products/prod-wf?change=wh-depot")
  const dialog = page.getByRole("dialog")
  await expect(dialog.getByLabel("Warehouse", { exact: true })).toHaveValue(
    depot.id
  )
  await expect(
    dialog.getByRole("button", { name: "Create General stock" })
  ).toHaveCount(0)

  await page.goto("/products/prod-wf?change=wh-missing")
  await expect(
    dialog.getByText("This warehouse is unavailable.", { exact: false })
  ).toBeVisible()
  await expect(
    dialog.getByRole("button", { name: "Save", exact: true })
  ).toBeDisabled()
  await dialog.getByLabel("Warehouse", { exact: true }).selectOption(store.id)
  await expect(
    dialog.getByRole("button", { name: "Save", exact: true })
  ).toBeEnabled()
  expect(api.callsTo("POST", warehousePath)).toEqual([])
})

test.describe(() => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/warehouses", status: 422 },
        { url: "/v1/workspace/w1/warehouses/wh-depot", status: 409 },
      ],
    },
  })

  test("General stock recovers an existing slug after simultaneous setup", async ({
    page,
    api,
  }) => {
    placesApi(api)
    const warehouseRows: (typeof generalStock)[] = []
    api
      .on("GET", warehousePath, () => paged(warehouseRows))
      .on("POST", warehousePath, () => {
        warehouseRows.push(generalStock)
        return {
          status: 422,
          body: { errors: { slug: ["That slug already exists."] } },
        }
      })

    await page.goto("/products/prod-wf?change=")
    const dialog = page.getByRole("dialog")
    await dialog.getByRole("button", { name: "Create General stock" }).click()
    await expect(dialog.getByLabel("Warehouse", { exact: true })).toHaveValue(
      generalStock.id
    )
    await expect(dialog.getByText("That slug already exists.")).toHaveCount(0)
    expect(api.callsTo("POST", warehousePath)).toHaveLength(1)
  })

  test("Office Stock links to warehouse create, edit and protected delete", async ({
    page,
    api,
  }) => {
    let warehouse = { ...depot, description: "", default_warehouse: true }
    let deleted = false
    let holdsStock = true
    api
      .on("GET", "/workspace/:ws/products/stocks", () => paged([]))
      .on("GET", warehousePath, ({ query }) =>
        paged(
          query.get("roots_only") === "false" && !deleted ? [warehouse] : []
        )
      )
      .on("POST", warehousePath, ({ body }) =>
        (body as { name: string }).name === depot.name
          ? {
              status: 422,
              body: { errors: { slug: ["That slug already exists."] } },
            }
          : { status: 201, body: { data: { warehouse_id: "wh-new" } } }
      )
      .on("POST", warehousePath + "/:warehouse", ({ body }) => {
        warehouse = {
          ...warehouse,
          ...(body as { name: string; description: string }),
        }
        return { message: "ok" }
      })
      .on("DELETE", warehousePath + "/:warehouse", () => {
        if (holdsStock)
          return { status: 409, body: { message: "warehouse_holds_stock" } }
        deleted = true
        return { message: "ok" }
      })

    await page.goto("/stock")
    await page.getByRole("link", { name: "Manage warehouses" }).click()
    await expect(
      page.getByRole("heading", { name: "Warehouses" })
    ).toBeVisible()
    await expect(
      page.getByRole("button", { name: "Create General stock" })
    ).toHaveCount(0)
    await expect(
      page.getByRole("row").filter({ hasText: depot.name })
    ).toContainText("Default")
    await page.getByRole("button", { name: "New warehouse" }).click()
    const form = page.getByRole("dialog")
    await form.getByLabel("Name", { exact: true }).fill(depot.name)
    await form.getByRole("button", { name: "Save", exact: true }).click()
    await expect(
      form.getByLabel("Name", { exact: true })
    ).toHaveAccessibleDescription("That slug already exists.")
    await form.getByLabel("Name", { exact: true }).fill("Cold store")
    await form.getByLabel("Description", { exact: true }).fill("For cold stock")
    await form.getByRole("button", { name: "Save", exact: true }).click()
    await expect(form).toBeHidden()
    await expect(page.getByText("Warehouse created.")).toBeVisible()
    expect(api.callsTo("POST", warehousePath).at(-1)?.body).toEqual({
      name: "Cold store",
      description: "For cold stock",
    })

    await page.getByRole("button", { name: "Edit", exact: true }).click()
    await form.getByLabel("Name", { exact: true }).fill("Depot")
    await form.getByRole("button", { name: "Save", exact: true }).click()
    const depotRow = page.getByRole("row").filter({ hasText: "Depot" })
    await expect(depotRow).toBeVisible()
    const confirmation = page.getByRole("alertdialog")
    const deleteDepot = depotRow.getByRole("button", {
      name: "Delete",
      exact: true,
    })
    await deleteDepot.click()
    await confirmation
      .getByRole("button", { name: "Cancel", exact: true })
      .click()
    expect(api.callsTo("DELETE", warehousePath + "/:warehouse")).toHaveLength(0)
    await expect(confirmation).toBeHidden()
    await deleteDepot.click()
    await confirmation
      .getByRole("button", { name: "Delete", exact: true })
      .click()
    await expect(
      page.getByText("This warehouse still has stock or reservations.", {
        exact: false,
      })
    ).toBeVisible()
    await expect(depotRow).toBeVisible()

    holdsStock = false
    await expect(confirmation).toBeHidden()
    await deleteDepot.click()
    await confirmation
      .getByRole("button", { name: "Delete", exact: true })
      .click()
    await expect(page.getByText("Warehouse deleted.")).toBeVisible()
    await expect(
      page.getByRole("button", { name: "Create General stock" })
    ).toBeVisible()
  })
})

test("Warehouse listing rights do not grant warehouse writes", async ({
  page,
  api,
}) => {
  api
    .on("GET", "/me", () => ({
      data: {
        user: {
          ...me,
          permissions: [{ workspace: WS, permissions: ["warehouse.list"] }],
        },
      },
    }))
    .on("GET", warehousePath, () => paged([depot]))
  await page.goto("/stock/warehouses")
  await expect(
    page.getByRole("row").filter({ hasText: depot.name })
  ).toBeVisible()
  await expect(
    page.getByRole("button", { name: /New warehouse|Edit|Delete/ })
  ).toHaveCount(0)
  await expect(
    page.getByRole("link", { name: "Stock", exact: true })
  ).toHaveCount(0)
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
