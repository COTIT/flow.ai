import { type Api, apiAs, cred, expect, loginOffice, stackAllow, test, unique, urls, workspaces } from "../../src/stack"

type StockRow = { stock_lvl: string; reserved: string; warehouse: { id: string } | null }
type Order = { id: string; status: string; warehouse_id: string; invoice_id: string | null }

/** The product's stock level in one warehouse, summed over its bins. */
async function level(api: Api, product: string, warehouse: string) {
  const rows = await api.get<StockRow[]>(api.ws(`/products/${product}/stock-with-bins`))
  return rows.filter((r) => r.warehouse?.id === warehouse).reduce((sum, r) => sum + Number(r.stock_lvl), 0)
}

/** A product, customer and draft order of the test's own: 3 jars from 10 in stock. */
async function draftOrder(owner: Api, quantity = "3") {
  const tag = unique()
  const { product_id: product } = await owner.post<{ product_id: string }>(owner.ws("/products"), {
    name: `e2e order jar ${tag}`,
    price: "5.00",
    has_stock: true,
  })
  const { relation_id } = await owner.post<{ relation_id: string }>(owner.ws("/relations"), {
    type: "business",
    company_name: `e2e customer ${tag}`,
  })
  const { order_id: id, number } = await owner.post<{ order_id: string; number: string }>(owner.ws("/orders"), {
    relation_id,
    lines: [{ product_id: product, quantity }],
  })
  const order = await owner.get<Order>(owner.ws(`/orders/${id}`))
  await owner.post(owner.ws(`/products/${product}/set-stock-levels`), {
    warehouses: [{ warehouse_id: order.warehouse_id, stock_lvl: "10.00", reason: "e2e" }],
  })
  return { id, number, product, warehouse: order.warehouse_id }
}

test("office order: confirm, pick, complete takes stock once; the invoice moves none", async ({ page, request }) => {
  const owner = await apiAs(request, cred("office (AFM)", "owner"))
  const o = await draftOrder(owner)
  const before = await level(owner, o.product, o.warehouse)

  await loginOffice(page, cred("office (AFM)", "owner"))
  await page.goto(`${urls.office}/orders/${o.id}`)
  await expect(page.getByRole("heading", { name: o.number })).toBeVisible()
  await page.getByRole("button", { name: "Confirm order" }).click()
  await expect(page.getByRole("status").filter({ hasText: "Order confirmed; stock is reserved." })).toBeVisible()
  // Reserving holds the jars but does not take them.
  expect(await level(owner, o.product, o.warehouse)).toBe(before)

  await page.getByRole("button", { name: "Record picking" }).click()
  await page.getByRole("dialog").getByRole("button", { name: "Record picking" }).click()
  await expect(page.getByRole("dialog")).toBeHidden()
  await page.getByRole("main").getByRole("button", { name: "Complete" }).click()
  await page.getByRole("alertdialog").getByRole("button", { name: "Complete" }).click()
  await expect(page.getByRole("status").filter({ hasText: "Order completed; stock taken." })).toBeVisible()
  await expect(page.getByText("Completed", { exact: true }).first()).toBeVisible()
  expect(await level(owner, o.product, o.warehouse)).toBe(before - 3)
  // Done once: the page offers no second Complete.
  await expect(page.getByRole("main").getByRole("button", { name: "Complete", exact: true })).toHaveCount(0)

  await page.getByRole("button", { name: "Make invoice" }).click()
  const open = page.getByRole("link", { name: "Open invoice" })
  await expect(open).toBeVisible()
  expect(await level(owner, o.product, o.warehouse)).toBe(before - 3)
  await expect(page.getByRole("button", { name: "Make invoice" })).toHaveCount(0)
  await open.click()
  await expect(page).toHaveURL(/\/invoices\/[0-9a-f-]{36}$/)
  await expect(page.getByRole("region", { name: "Lines" })).toContainText(`e2e order jar`)

  // The API refuses a second complete and a second invoice; stock stays where it was.
  await expect(owner.post(owner.ws(`/orders/${o.id}/complete`))).rejects.toThrow(/: 409|: 422/)
  await expect(owner.post(owner.ws(`/orders/${o.id}/invoice`))).rejects.toThrow(/: 409|: 422/)
  expect(await level(owner, o.product, o.warehouse)).toBe(before - 3)
})

test.describe(() => {
  // The order is made before the test, so the allowed 422 can name it (allow is read as the test starts).
  let o: Awaited<ReturnType<typeof draftOrder>>
  test.beforeAll(async ({ request }) => {
    o = await draftOrder(await apiAs(request, cred("office (AFM)", "owner")), "12")
  })
  test.use({
    // eslint-disable-next-line no-empty-pattern -- Playwright reads fixture deps from the pattern
    allow: async ({}, use) =>
      use({
        ...stackAllow,
        responses: [
          ...stackAllow.responses,
          { url: `${urls.office}/v1/workspace/${workspaces.AFM}/orders/${o.id}/confirm`, status: 422 },
        ],
      }),
  })

  test("office order: confirming more than is in stock shows the API's answer and reserves nothing", async ({
    page,
    request,
  }) => {
    const owner = await apiAs(request, cred("office (AFM)", "owner"))
    await loginOffice(page, cred("office (AFM)", "owner"))
    await page.goto(`${urls.office}/orders/${o.id}`)
    await page.getByRole("button", { name: "Confirm order" }).click()
    await expect(page.getByRole("alert").filter({ hasText: "Not enough stock: 10 available." })).toBeVisible()
    expect((await owner.get<Order>(owner.ws(`/orders/${o.id}`))).status).toBe("draft")
    await expect(page.getByRole("button", { name: "Confirm order" })).toBeVisible()
  })
})
