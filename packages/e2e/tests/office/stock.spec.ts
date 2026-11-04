import type { Page } from "@playwright/test"

import { expect, expectFits, type FakeApi, paged, test } from "./fake-api"

const product = {
  id: "prod-honey",
  name: "AFM Honey 500g",
  reference: "AFM-500",
  price: "10.33",
  status: "active",
  short_description: null,
  description: null,
  store_only: false,
  labels: ["Raw", "Unheated"],
  total_available_stock: 120,
  total_reserved_stock: 0,
  last_updated_at: "2026-10-01T08:00:00Z",
}
const warehouse = { id: "wh-1", name: "Dar es Salaam" }

function stockApi(api: FakeApi) {
  const rows = [
    {
      id: "st-1",
      stock_lvl: "120.00",
      reserved: "4.00",
      minimal_stock_level: 10,
      maximum_stock_level: 0,
      warehouse,
      shelf: null,
      bin: null,
    },
  ]
  const movements: Record<string, unknown>[] = []
  api
    .on("GET", "/workspace/:ws/products/:id", () => ({ data: product }))
    .on("POST", "/workspace/:ws/products/:id", ({ body }) => {
      const b = body as Record<string, unknown>
      if (b.price === "abc")
        return {
          status: 422,
          body: {
            message: "The given data was invalid.",
            errors: { price: ["The price must be a number."] },
          },
        }
      Object.assign(product, b, { last_updated_at: new Date().toISOString() })
      return { data: product }
    })
    .on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => ({
      data: rows,
    }))
    .on("GET", "/workspace/:ws/products/:id/stock-movements", ({ query }) => {
      // The API filters by `type`; the fake keeps rows of the asked types.
      const types = query.get("type")?.split(",")
      return paged(
        types ? movements.filter((m) => types.includes(String(m.movement_type))) : movements
      )
    })
    .on("GET", "/workspace/:ws/warehouses", () => paged([warehouse]))
    .on("GET", "/workspace/:ws/products/:id/images", () => ({ data: [] }))
    .on("GET", "/workspace/:ws/files", () => paged([]))
    .on("POST", "/workspace/:ws/products/:id/stocks", ({ body }) => {
      const b = body as { mutation_value: string; reason: string | null }
      if (b.mutation_value === "-500")
        return {
          status: 422,
          body: { message: "Not enough stock.", errors: {} },
        }
      rows[0]!.stock_lvl = "130.00"
      movements.unshift({
        id: "mv-1",
        // The API books a change as an increase or a decrease.
        movement_type: b.mutation_value.startsWith("-") ? "decrease" : "increase",
        quantity_change: b.mutation_value,
        quantity_before: "120.00",
        quantity_after: "130.00",
        reason: b.reason,
        created_at: "2026-10-31T10:00:00Z",
      })
      return { data: rows[0] }
    })
}

test("edit product: only the form's keys are sent", async ({ page, api }) => {
  stockApi(api)
  await page.goto("/products/prod-honey")
  await page.getByLabel("Short description").fill("Raw forest honey")
  await page.getByRole("button", { name: "Save" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "Product saved." })
  ).toBeVisible()
  expect(api.callsTo("POST", "/workspace/:ws/products/:id")[0]?.body).toEqual({
    name: "AFM Honey 500g",
    reference: "AFM-500",
    price: "10.33",
    status: "active",
    short_description: "Raw forest honey",
    description: null,
    store_only: false,
    labels: ["Raw", "Unheated"],
  })
})

test("labels: comma-separated in the form, sent as a list; empty clears them", async ({
  page,
  api,
}) => {
  stockApi(api)
  await page.goto("/products/prod-honey")
  const labels = page.getByLabel("Labels")
  await expect(labels).toHaveValue("Raw, Unheated")
  await expect(
    page.getByText("Shown as chips in the shop, up to 8. Separate them with commas.")
  ).toBeVisible()
  await labels.fill(" Raw ,Single origin,, Unheated ")
  await page.getByRole("button", { name: "Save" }).click()
  await expect(page.getByRole("status").filter({ hasText: "Product saved." })).toBeVisible()
  expect(api.callsTo("POST", "/workspace/:ws/products/:id")[0]?.body).toMatchObject({
    labels: ["Raw", "Single origin", "Unheated"],
  })
  await expect(labels).toHaveValue("Raw, Single origin, Unheated")

  await labels.fill("")
  await page.getByRole("button", { name: "Save" }).click()
  await expect.poll(() => api.callsTo("POST", "/workspace/:ws/products/:id").length).toBe(2)
  expect(api.callsTo("POST", "/workspace/:ws/products/:id")[1]?.body).toMatchObject({
    labels: [],
  })
  await expectFits(page)
})

test.describe(() => {
  test.use({
    allow: {
      responses: [{ url: "/v1/workspace/w1/products/prod-honey", status: 422 }],
    },
  })

  test("labels: the API's 422 shows at the field", async ({ page, api }) => {
    stockApi(api)
    api.on("POST", "/workspace/:ws/products/:id", () => ({
      status: 422,
      body: {
        message: "The given data was invalid.",
        errors: { labels: ["At most 8 labels."] },
      },
    }))
    await page.goto("/products/prod-honey")
    await page.getByLabel("Labels").fill("a, b, c, d, e, f, g, h, i")
    await page.getByRole("button", { name: "Save" }).click()
    await expect(page.getByText("At most 8 labels.")).toBeVisible()
    await expect(page.getByLabel("Labels")).toHaveAttribute("aria-invalid", "true")
  })
})

test("stock movements: All / Decreases / Counts ask the API for that type", async ({
  page,
  api,
}) => {
  stockApi(api)
  const base = {
    quantity_before: "10.00",
    quantity_after: "9.00",
    reason: null,
    relationships: { warehouse },
  }
  api.on("GET", "/workspace/:ws/products/:id/stock-movements", ({ query }) => {
    const all = [
      { ...base, id: "mv-d", movement_type: "decrease", quantity_change: "-1.00", created_at: "2026-11-02T09:00:00Z" },
      { ...base, id: "mv-s", movement_type: "set_level", quantity_change: "-3.00", created_at: "2026-11-01T09:00:00Z" },
      { ...base, id: "mv-i", movement_type: "increase", quantity_change: "120.00", created_at: "2026-10-30T09:00:00Z" },
    ]
    const type = query.get("type")
    return paged(type ? all.filter((m) => m.movement_type === type) : all)
  })
  await page.goto("/products/prod-honey")
  const card = page.getByRole("region", { name: "Stock movements" })
  const kind = card.getByRole("group", { name: "Movement type" })
  await expect(kind.getByRole("button", { name: "All" })).toHaveAttribute("aria-pressed", "true")
  await expect(card.getByRole("cell", { name: "120.00" })).toBeVisible()

  await kind.getByRole("button", { name: "Decreases" }).click()
  await expect(kind.getByRole("button", { name: "Decreases" })).toHaveAttribute("aria-pressed", "true")
  await expect(card.getByRole("cell", { name: "120.00" })).toHaveCount(0)
  await expect(card.getByRole("cell", { name: "-1.00" })).toBeVisible()

  await kind.getByRole("button", { name: "Counts" }).click()
  await expect(card.getByRole("cell", { name: "-3.00" })).toBeVisible()
  await expect(card.getByRole("cell", { name: "-1.00" })).toHaveCount(0)
  await expectFits(page)
})

test("stock movements: the type goes out as the API's word", async ({ page, api }) => {
  stockApi(api)
  const seen: (string | null)[] = []
  await page.route("**/v1/workspace/w1/products/prod-honey/stock-movements**", (route) => {
    seen.push(new URL(route.request().url()).searchParams.get("type"))
    return route.fallback()
  })
  await page.goto("/products/prod-honey")
  const kind = page.getByRole("group", { name: "Movement type" })
  await kind.getByRole("button", { name: "Decreases" }).click()
  await kind.getByRole("button", { name: "Counts" }).click()
  await expect.poll(() => seen).toEqual([null, "decrease", "set_level"])
})

test("store only: ticking it sends store_only true", async ({ page, api }) => {
  stockApi(api)
  await page.goto("/products/prod-honey")
  await page.getByRole("checkbox", { name: "Sold at the till only (not in the web shop)" }).check()
  await page.getByRole("button", { name: "Save" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "Product saved." })
  ).toBeVisible()
  expect(
    api.callsTo("POST", "/workspace/:ws/products/:id")[0]?.body
  ).toMatchObject({
    store_only: true,
  })
  await expect(
    page.getByRole("checkbox", { name: "Sold at the till only (not in the web shop)" })
  ).toBeChecked()
})

test.describe(() => {
  test.use({
    allow: {
      responses: [{ url: "/v1/workspace/w1/products/prod-honey", status: 422 }],
    },
  })

  test("edit product: 422 shows at the price", async ({ page, api }) => {
    stockApi(api)
    await page.goto("/products/prod-honey")
    await page.getByLabel("Price excl. VAT").fill("abc")
    await page.getByRole("button", { name: "Save" }).click()
    await expect(page.getByText("The price must be a number.")).toBeVisible()
  })
})

test("stock per warehouse; a change shows as a movement", async ({
  page,
  api,
}) => {
  stockApi(api)
  await page.goto("/products/prod-honey")
  const stock = page.getByRole("region", { name: "Stock", exact: true })
  await expect(stock.getByText("120.00")).toBeVisible()
  await expect(stock.getByText("120", { exact: true })).toBeVisible()
  await page.getByRole("button", { name: "Change stock" }).click()
  const dialog = page.getByRole("dialog")
  await expect(dialog.getByLabel("Warehouse")).toHaveValue("wh-1")
  await dialog.getByLabel("Change (e.g. 10 or -3)").fill("10")
  await dialog.getByLabel("Reason").fill("Harvest intake")
  await dialog.getByRole("button", { name: "Save" }).click()
  await expect(stock.getByText("130.00")).toBeVisible()
  // The movement: its change shows on a phone card too; the reason only on wider screens.
  await expect(page.getByRole("cell", { name: "10", exact: true })).toBeVisible()
  await expect(page.getByText("Harvest intake")).toBeAttached()
  await expect(
    page.getByRole("status").filter({ hasText: "Stock changed" })
  ).toBeVisible()
  expect(
    api.callsTo("POST", "/workspace/:ws/products/:id/stocks")[0]?.body
  ).toEqual({
    warehouse_id: "wh-1",
    mutation_value: "10",
    reason: "Harvest intake",
  })
  await expectFits(page)
})

test.describe(() => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/products/prod-honey/stocks", status: 422 },
      ],
    },
  })

  test("stock change refused: the API's message shows", async ({
    page,
    api,
  }) => {
    stockApi(api)
    await page.goto("/products/prod-honey")
    await page.getByRole("button", { name: "Change stock" }).click()
    // The form sends to a warehouse: wait until they are read.
    await expect(page.getByRole("dialog").getByLabel("Warehouse")).toHaveValue("wh-1")
    await page
      .getByRole("dialog")
      .getByLabel("Change (e.g. 10 or -3)")
      .fill("-500")
    await page.getByRole("dialog").getByRole("button", { name: "Save" }).click()
    await expect(
      page.getByRole("alert").filter({ hasText: "Not enough stock." })
    ).toBeVisible()
  })
})

test("all stock and below-zero lists", async ({ page, api }) => {
  const row = (id: string, lvl: string) => ({
    id,
    stock_lvl: lvl,
    reserved: 0,
    minimal_stock_level: 0,
    maximum_stock_level: 0,
    meta: null,
    relationships: { product, warehouse },
  })
  // The product page the last click opens.
  stockApi(api)
  api
    .on("GET", "/workspace/:ws/products/stocks", () =>
      paged([row("s1", "120.00")])
    )
    .on("GET", "/workspace/:ws/products/stocks/negative", () =>
      paged([row("s2", "-3.00")])
    )
  await page.goto("/stock")
  await expect(page.getByRole("cell", { name: "120.00" })).toBeVisible()
  await page.getByRole("link", { name: "Below zero" }).click()
  await expect(page).toHaveURL(/\/stock\/negative$/)
  await expect(page.getByRole("cell", { name: "-3.00" })).toBeVisible()
  await expect(page.getByRole("link", { name: "Below zero" })).toHaveAttribute(
    "aria-current",
    "page"
  )
  await page.getByRole("link", { name: "AFM Honey 500g" }).click()
  await expect(page).toHaveURL(/\/products\/prod-honey$/)
  await expect(
    page.getByRole("heading", { name: "AFM Honey 500g" })
  ).toBeVisible()
})

test("/products/[id] edit: a new price and status are sent as typed and shown after the save", async ({
  page,
  api,
}) => {
  stockApi(api)
  await page.goto("/products/prod-honey")
  await page.getByLabel("Price excl. VAT").fill("12.40")
  await page.getByLabel("Status").selectOption({ label: "No longer sold" })
  await page.getByRole("button", { name: "Save" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "Product saved." })
  ).toBeVisible()
  expect(api.callsTo("POST", "/workspace/:ws/products/:id")[0]?.body).toMatchObject({
    price: "12.40",
    status: "nla",
  })
  // The page reads the product again: the badge and the field show the API's answer.
  await expect(page.getByText("No longer sold", { exact: true }).first()).toBeVisible()
  await expect(page.getByLabel("Price excl. VAT")).toHaveValue("12.40")
  await expect(page.getByLabel("Status")).toHaveValue("nla")
})

test("/products/[id] edit: a very long name saves and the page still fits", async ({
  page,
  api,
}) => {
  stockApi(api)
  const long = "Ilundo raw forest honey from the miombo woodland ".repeat(5).trim()
  await page.goto("/products/prod-honey")
  await page.getByLabel("Name").fill(long)
  await page.getByRole("button", { name: "Save" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "Product saved." })
  ).toBeVisible()
  expect(api.callsTo("POST", "/workspace/:ws/products/:id")[0]?.body).toMatchObject({
    name: long,
  })
  await expect(page.getByRole("heading", { name: long })).toBeVisible()
  await expectFits(page)
})

// The product page's two writes, and how to start each.
const productWrites = [
  {
    write: "edit",
    path: "/workspace/:ws/products/:id",
    url: "/v1/workspace/w1/products/prod-honey",
    run: async (page: Page) => {
      await page.getByLabel("Price excl. VAT").fill("11.00")
      await page.getByRole("button", { name: "Save" }).click()
    },
    saved: "Product saved.",
  },
  {
    write: "stock change",
    path: "/workspace/:ws/products/:id/stocks",
    url: "/v1/workspace/w1/products/prod-honey/stocks",
    run: async (page: Page) => {
      const dialog = page.getByRole("dialog")
      if (!(await dialog.isVisible()))
        await page.getByRole("button", { name: "Change stock" }).click()
      await expect(dialog.getByLabel("Warehouse")).toHaveValue("wh-1")
      await dialog.getByLabel("Change (e.g. 10 or -3)").fill("10")
      await dialog.getByRole("button", { name: "Save" }).click()
    },
    saved: "Stock changed",
  },
]

for (const w of productWrites)
  for (const { status, message } of [
    { status: 403, message: "You may not change products." },
    { status: 500, message: "The server could not finish this." },
  ])
    test.describe(() => {
      test.use({ allow: { responses: [{ url: w.url, status }] } })

      test(`/products/[id] ${w.write}: the API's ${status} shows; nothing marked saved`, async ({
        page,
        api,
      }) => {
        stockApi(api)
        api.on("POST", w.path, () => ({ status, body: { message } }))
        await page.goto("/products/prod-honey")
        await w.run(page)
        await expect(
          page.getByRole("alert").filter({ hasText: message }),
          `/products/[id] ${w.write} ${status}`
        ).toBeVisible()
        await expect(page.getByRole("status").filter({ hasText: w.saved })).toHaveCount(0)
        expect(api.callsTo("POST", w.path)).toHaveLength(1)
      })
    })

test("/products/[id] edit: a network failure says so; saving again sends the same form", async ({
  page,
  api,
}) => {
  stockApi(api)
  let fail = true
  await page.route("**/v1/workspace/w1/products/prod-honey", async (route) => {
    if (route.request().method() === "POST" && fail) {
      fail = false
      return route.abort("aborted")
    }
    return route.fallback()
  })
  await page.goto("/products/prod-honey")
  await page.getByLabel("Price excl. VAT").fill("11.00")
  await page.getByRole("button", { name: "Save" }).click()
  await expect(
    page.getByRole("alert").filter({ hasText: "Something went wrong. Try again." })
  ).toBeVisible()
  await expect(page.getByLabel("Price excl. VAT")).toHaveValue("11.00")
  await page.getByRole("button", { name: "Save" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "Product saved." })
  ).toBeVisible()
  // Saving a product sets fields, so a repeat is harmless: one save reached the API.
  const saves = api.callsTo("POST", "/workspace/:ws/products/:id")
  expect(saves).toHaveLength(1)
  expect(saves[0]?.body).toMatchObject({ price: "11.00" })
})

test("/products/[id] stock change: retried after a network failure, the API can tell it is the same change", async ({
  page,
  api,
}) => {
  test.fail(
    true,
    "BUG: a stock change is sent without an Idempotency-Key (apps/office/app/(app)/products/[id]/page.tsx:501), and POST /products/{p}/stocks takes none, so a retry after a lost reply moves stock twice"
  )
  stockApi(api)
  const keys: string[] = []
  let fail = true
  await page.route("**/v1/workspace/w1/products/prod-honey/stocks", async (route) => {
    keys.push(route.request().headers()["idempotency-key"] ?? "")
    if (fail) {
      fail = false
      return route.abort("aborted")
    }
    return route.fallback()
  })
  await page.goto("/products/prod-honey")
  await productWrites[1]!.run(page)
  await expect(
    page.getByRole("alert").filter({ hasText: "Something went wrong. Try again." })
  ).toBeVisible()
  await productWrites[1]!.run(page)
  await expect(
    page.getByRole("status").filter({ hasText: "Stock changed" })
  ).toBeVisible()
  expect(keys).toHaveLength(2)
  expect(keys[0]).toMatch(/^[0-9a-f-]{36}$/)
  expect(keys[1]).toBe(keys[0])
})
