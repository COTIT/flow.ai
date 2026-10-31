import type { StockMovement, StockWithBins } from "@genesy/api-client/model"

import { expect, paged, test, type FakeApi } from "./fixtures"

const product = {
  id: "prod-honey",
  name: "AFM Honey 500g",
  reference: "AFM-500",
  price: "10.33",
  status: "active",
  short_description: null,
  description: null,
  last_updated_at: "2026-10-01T08:00:00Z",
}
const warehouse = { id: "wh-1", name: "Dar es Salaam" }

function stockApi(api: FakeApi) {
  const rows: StockWithBins[] = [
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
  const movements: StockMovement[] = []
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
    .on("GET", "/workspace/:ws/products/:id/stock-movements", () =>
      paged(movements)
    )
    .on("GET", "/workspace/:ws/warehouses", () => paged([warehouse]))
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
        movement_type: "mutation",
        quantity_change: b.mutation_value,
        quantity_before: "120.00",
        quantity_after: "130.00",
        reason: b.reason,
        created_at: "2026-10-31T10:00:00Z",
      } as StockMovement)
      return { data: rows[0] }
    })
}

test("edit product: only the form's keys are sent", async ({ page, api }) => {
  stockApi(api)
  await page.goto("/products/prod-honey")
  await page.getByLabel("Short description").fill("Raw forest honey")
  await page.getByRole("button", { name: "Save" }).click()
  await expect(page.getByRole("status")).toHaveText("Saved.")
  expect(api.callsTo("POST", "/workspace/:ws/products/:id")[0]?.body).toEqual({
    name: "AFM Honey 500g",
    reference: "AFM-500",
    price: "10.33",
    status: "active",
    short_description: "Raw forest honey",
    description: null,
  })
})

test("edit product: 422 shows at the price", async ({ page, api }) => {
  stockApi(api)
  await page.goto("/products/prod-honey")
  await page.getByLabel("Price excl. VAT").fill("abc")
  await page.getByRole("button", { name: "Save" }).click()
  await expect(page.getByText("The price must be a number.")).toBeVisible()
})

test("stock per warehouse; a change shows as a movement", async ({
  page,
  api,
}) => {
  stockApi(api)
  await page.goto("/products/prod-honey")
  await expect(page.getByRole("cell", { name: "120.00" })).toBeVisible()
  await page.getByRole("button", { name: "Change stock" }).click()
  const dialog = page.getByRole("dialog")
  await expect(dialog.getByLabel("Warehouse")).toHaveValue("wh-1")
  await dialog.getByLabel("Change (e.g. 10 or -3)").fill("10")
  await dialog.getByLabel("Reason").fill("Harvest intake")
  await dialog.getByRole("button", { name: "Save" }).click()
  await expect(page.getByRole("cell", { name: "130.00" }).first()).toBeVisible()
  await expect(page.getByRole("cell", { name: "Harvest intake" })).toBeVisible()
  expect(
    api.callsTo("POST", "/workspace/:ws/products/:id/stocks")[0]?.body
  ).toEqual({
    warehouse_id: "wh-1",
    mutation_value: "10",
    reason: "Harvest intake",
  })
})

test("stock change refused: the API's message shows", async ({ page, api }) => {
  stockApi(api)
  await page.goto("/products/prod-honey")
  await page.getByRole("button", { name: "Change stock" }).click()
  await page
    .getByRole("dialog")
    .getByLabel("Change (e.g. 10 or -3)")
    .fill("-500")
  await page.getByRole("dialog").getByRole("button", { name: "Save" }).click()
  await expect(page.getByRole("alert")).toHaveText("Not enough stock.")
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
  await page.getByRole("link", { name: "AFM Honey 500g" }).click()
  await expect(page).toHaveURL(/\/products\/prod-honey$/)
})
