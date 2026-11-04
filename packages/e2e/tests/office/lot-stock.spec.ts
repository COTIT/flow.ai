import { expect, expectFits, paged, test, type FakeApi } from "./fake-api"

const lot = {
  id: "lot-1",
  product_id: "prod-1",
  code: "AFM-RAW-01",
  public_code: "AB12CD34EF",
  status: "released",
  produced_on: null,
  best_before: null,
  expires_on: null,
  origin: null,
  created_at: "2026-11-01T08:00:00Z",
  updated_at: "2026-11-01T08:00:00Z",
}

type Row = { warehouse_id: string; level: string; reserved: string; available: string }

function lotApi(
  api: FakeApi,
  rows: Row[] = [
    { warehouse_id: "wh-1", level: "248.50", reserved: "10.00", available: "238.50" },
  ]
) {
  const stock = [...rows]
  api
    .on("GET", "/workspace/:ws/lots/:id", () => ({ data: lot }))
    .on("GET", "/workspace/:ws/lots/:id/events", () => paged([]))
    .on("GET", "/workspace/:ws/products/:id", () => ({
      data: { id: "prod-1", name: "AFM raw honey" },
    }))
    .on("GET", "/workspace/:ws/lots/:id/passport", () => ({
      data: { draft: {}, published: null, published_at: null, version: 0 },
    }))
    .on("GET", "/workspace/:ws/lots/:id/stock", () => ({ data: stock }))
    .on("GET", "/workspace/:ws/warehouses", () =>
      paged([
        { id: "wh-1", name: "Dar es Salaam" },
        { id: "wh-2", name: "Arusha" },
      ])
    )
    .on("POST", "/workspace/:ws/lots/:id/stock-mutations", ({ body }) => {
      const b = body as { warehouse_id: string; quantity: string }
      if (b.quantity === "-999")
        return {
          status: 422,
          body: {
            message: "The given data was invalid.",
            errors: { quantity: ["Only 238.50 is free in this warehouse."] },
          },
        }
      // The fake's own bookkeeping, standing in for the API's answer.
      stock[0] = { warehouse_id: "wh-1", level: "250.00", reserved: "10.00", available: "240.00" }
      return { status: 201, body: { data: { warehouse_id: b.warehouse_id, stock_lvl: "250.00" } } }
    })
}

test("change stock shows the stock per warehouse", async ({ page, api }) => {
  lotApi(api)
  await page.goto("/lots/lot-1")
  await page.getByRole("button", { name: "Change stock" }).click()
  const dialog = page.getByRole("dialog")
  const tile = dialog.getByRole("group", { name: "Dar es Salaam" })
  await expect(tile).toContainText("248.50")
  await expect(tile).toContainText("In stock")
  await expect(tile).toContainText("10.00")
  await expect(tile).toContainText("Reserved")
  await expect(tile).toContainText("238.50")
  await expect(tile).toContainText("Available")
  await expect(dialog.getByText("Positive adds, negative takes out")).toBeVisible()
  await expectFits(page)
})

test("an empty batch says so", async ({ page, api }) => {
  lotApi(api, [])
  await page.goto("/lots/lot-1")
  await page.getByRole("button", { name: "Change stock" }).click()
  await expect(
    page.getByRole("dialog").getByText("No stock in this batch yet")
  ).toBeVisible()
})

test("change stock: body exact, notice shown, stock and history read again", async ({
  page,
  api,
}) => {
  lotApi(api)
  await page.goto("/lots/lot-1")
  await page.getByRole("button", { name: "Change stock" }).click()
  const dialog = page.getByRole("dialog")
  await expect(dialog.getByRole("group", { name: "Dar es Salaam" })).toBeVisible()
  await dialog.getByLabel("Warehouse").selectOption({ label: "Arusha" })
  await dialog.getByLabel("Quantity").fill("1.50")
  await dialog.getByLabel("Reason").fill("Counted")
  await dialog.getByRole("checkbox", { name: "This is a correction" }).click()
  const events = api.callsTo("GET", "/workspace/:ws/lots/:id/events").length
  await dialog.getByRole("button", { name: "Save" }).click()
  await expect(dialog).toBeHidden()
  expect(
    api.callsTo("POST", "/workspace/:ws/lots/:id/stock-mutations")[0]?.body
  ).toEqual({
    warehouse_id: "wh-2",
    quantity: "1.50",
    reason: "Counted",
    is_correction: true,
  })
  await expect(
    page.getByRole("status").filter({ hasText: "Stock changed." })
  ).toBeVisible()
  await expect
    .poll(() => api.callsTo("GET", "/workspace/:ws/lots/:id/events").length)
    .toBeGreaterThan(events)
  await page.getByRole("button", { name: "Change stock" }).click()
  await expect(
    page.getByRole("dialog").getByRole("group", { name: "Dar es Salaam" })
  ).toContainText("250.00")
})

test("no reason and no correction send null and false", async ({ page, api }) => {
  lotApi(api)
  await page.goto("/lots/lot-1")
  await page.getByRole("button", { name: "Change stock" }).click()
  const dialog = page.getByRole("dialog")
  await expect(dialog.getByLabel("Warehouse")).toContainText("Dar es Salaam")
  await dialog.getByLabel("Quantity").fill("5")
  await dialog.getByRole("button", { name: "Save" }).click()
  await expect(dialog).toBeHidden()
  expect(
    api.callsTo("POST", "/workspace/:ws/lots/:id/stock-mutations")[0]?.body
  ).toEqual({ warehouse_id: "wh-1", quantity: "5", reason: null, is_correction: false })
})

test.describe(() => {
  test.use({
    allow: {
      responses: [{ url: "/v1/workspace/w1/lots/lot-1/stock-mutations", status: 422 }],
    },
  })

  test("taking out more than is free shows the API's 422", async ({ page, api }) => {
    lotApi(api)
    await page.goto("/lots/lot-1")
    await page.getByRole("button", { name: "Change stock" }).click()
    const dialog = page.getByRole("dialog")
    await expect(dialog.getByLabel("Warehouse")).toContainText("Dar es Salaam")
    await dialog.getByLabel("Quantity").fill("-999")
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(dialog.getByText("Only 238.50 is free in this warehouse.")).toBeVisible()
    await expect(dialog.getByLabel("Quantity")).toHaveAttribute("aria-invalid", "true")
  })
})
