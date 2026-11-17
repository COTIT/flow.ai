import type { APIRequestContext } from "@playwright/test"

import { expectNoRawValues } from "../../src/test"
import { type Api, apiAs, cred, expect, loginOffice, stackAllow, test, unique, urls, workspaces } from "../../src/stack"

type StockRow = { stock_lvl: string; warehouse: { id: string } | null }
type Po = { id: string; number: number; version: number; receipt_status: string; lines: { id: string; open_quantity: string }[] }
const allow = [/\bPO-\d+\b/, /\bGR-\d+\b/]

/** The product's stock level in one warehouse, summed over its bins. */
async function level(api: Api, product: string, warehouse: string) {
  const rows = await api.get<StockRow[]>(api.ws(`/products/${product}/stock-with-bins`))
  return rows.filter((r) => r.warehouse?.id === warehouse).reduce((sum, r) => sum + Number(r.stock_lvl), 0)
}

/** POST with the Idempotency-Key the purchasing API requires (apiAs sends none). */
async function command<T>(request: APIRequestContext, owner: Api, path: string, payload: unknown) {
  const key = crypto.randomUUID()
  const res = await request.post(`${urls.api}/v1${owner.ws(path)}`, {
    headers: { Authorization: `Bearer ${owner.token}`, "Idempotency-Key": key },
    data: { client_ref: key, payload },
  })
  if (res.status() >= 400) throw new Error(`POST ${path}: ${res.status()} ${await res.text()}`)
  return ((await res.json()) as { data: T }).data
}

/** A supplier (relation of a supplier type), a stock product, and the default warehouse. */
async function world(owner: Api) {
  const tag = unique()
  const types = await owner.get<{ id: string }[]>(owner.ws("/custom-types?domain=relation&is_supplier=1"))
  const { relation_id: supplier } = await owner.post<{ relation_id: string }>(owner.ws("/relations"), {
    type: "business",
    company_name: `e2e supplier ${tag}`,
    relation_type_id: types[0]!.id,
  })
  const { product_id: product } = await owner.post<{ product_id: string }>(owner.ws("/products"), {
    name: `e2e bought jar ${tag}`,
    price: "5.00",
    has_stock: true,
  })
  const warehouses = await owner.get<{ id: string; default_warehouse: boolean }[]>(owner.ws("/warehouses?page_size=100"))
  const warehouse = (warehouses.find((w) => w.default_warehouse) ?? warehouses[0]!).id
  const currencies = await owner.get<{ id: string; default_currency: boolean }[]>(owner.ws("/currencies?page_size=100"))
  const currency = currencies.find((c) => c.default_currency)!.id
  return { tag, supplier, product, warehouse, currency }
}

test.describe(() => {
  // Made before the test, so the allowed 422 (receiving more than is open) can name the order.
  let w: Awaited<ReturnType<typeof world>>
  let po: Po
  test.beforeAll(async ({ request }) => {
    const owner = await apiAs(request, cred("office (AFM)", "owner"))
    w = await world(owner)
    po = await command<Po>(request, owner, "/purchase-orders", {
      supplier_id: w.supplier,
      currency_id: w.currency,
      purchase_date: new Date().toISOString().slice(0, 10),
      default_warehouse_id: w.warehouse,
      lines: [{ product_id: w.product, quantity: "5", unit_price: "4.00" }],
    })
  })
  test.use({
    // eslint-disable-next-line no-empty-pattern -- Playwright reads fixture deps from the pattern
    allow: async ({}, use) =>
      use({
        ...stackAllow,
        responses: [
          ...stackAllow.responses,
          { url: `${urls.office}/v1/workspace/${workspaces.AFM}/purchase-orders/${po.id}/receipts`, status: 422 },
        ],
      }),
  })

  test("purchase order: order 5, receive 2, refuse 4 of 3 open, receive 3, book each receipt once", async ({
    page,
    request,
  }) => {
    const owner = await apiAs(request, cred("office (AFM)", "owner"))
    const before = await level(owner, w.product, w.warehouse)
    await loginOffice(page, cred("office (AFM)", "owner"))
    await page.goto(`${urls.office}/purchases/${po.id}`)
    await expect(page.getByRole("heading", { name: `PO-${po.number}` })).toBeVisible()

    const next = page.getByRole("region", { name: "Next step" })
    await next.getByRole("button", { name: "Confirm order" }).click()
    let dialog = page.getByRole("dialog")
    await expect(dialog).toContainText(`e2e supplier ${w.tag}`)
    await dialog.getByRole("button", { name: "Confirm order" }).click()
    await expect(page.getByText("Order confirmed.")).toBeVisible()

    const receive = async (qty: string) => {
      await next.getByRole("button", { name: "Receive goods" }).click()
      dialog = page.getByRole("dialog")
      await dialog.getByLabel(/e2e bought jar/).fill(qty)
      await dialog.getByRole("button", { name: "Receive goods" }).click()
    }
    await receive("2")
    await expect(page.getByText(/Receipt GR-\d+ recorded\./)).toBeVisible()
    // Received, not booked: stock has not moved yet.
    expect(await level(owner, w.product, w.warehouse)).toBe(before)

    await receive("4")
    await expect(dialog.locator("#rc-q-" + po.lines[0]!.id + "-error")).toBeVisible()
    await expectNoRawValues(page, { allow })
    await dialog.getByLabel(/e2e bought jar/).fill("3")
    await dialog.getByRole("button", { name: "Receive goods" }).click()
    await expect(dialog).toBeHidden()
    await expect(page.getByRole("status").filter({ hasText: /Receipt GR-\d+ recorded\./ })).toBeVisible()
    await expect(page.getByText("Received", { exact: true }).first()).toBeVisible()

    // Book the first receipt, then the second, each through the server's review.
    const receipts = page.getByRole("region", { name: "Receipts" }).getByRole("link")
    await expect(receipts).toHaveCount(2)
    const links = await receipts.evaluateAll((as) => as.map((a) => (a as HTMLAnchorElement).href))
    for (const href of [...links].reverse()) {
      await page.goto(href)
      await page.getByRole("button", { name: "Book into stock" }).click()
      dialog = page.getByRole("dialog")
      await dialog.getByRole("button", { name: "Review" }).click()
      // A setup book (before accounting is active) books stock only.
      await expect(dialog.getByText(/Stock only/)).toBeVisible()
      await dialog.getByRole("button", { name: "Book into stock" }).click()
      await expect(page.getByText("Booked into stock.")).toBeVisible()
      await expect(page.getByRole("button", { name: "Book into stock" })).toHaveCount(0)
      await expectNoRawValues(page, { allow })
    }
    expect(await level(owner, w.product, w.warehouse)).toBe(before + 5)

    // Booked once: the API refuses to receive more and offers no second booking.
    const after = await owner.get<Po & { allowed_actions: string[] }>(owner.ws(`/purchase-orders/${po.id}`))
    expect(after.receipt_status).toBe("received")
    expect(after.allowed_actions).not.toContain("receive")
    expect(await level(owner, w.product, w.warehouse)).toBe(before + 5)
  })
})

test("purchase order: a draft made in Office, part received, the rest closed, the receipt still books", async ({
  page,
  request,
}) => {
  const owner = await apiAs(request, cred("office (AFM)", "owner"))
  const w = await world(owner)
  const before = await level(owner, w.product, w.warehouse)
  await loginOffice(page, cred("office (AFM)", "owner"))
  await page.goto(`${urls.office}/purchases/new`)
  await page.getByLabel("Supplier", { exact: true }).selectOption({ label: `e2e supplier ${w.tag}` })
  await page.getByLabel("Deliver to").selectOption(w.warehouse)
  await page.getByLabel("Product").selectOption({ label: `e2e bought jar ${w.tag}` })
  await page.getByLabel("Quantity", { exact: true }).fill("3")
  await page.getByLabel("Unit price").fill("2.50")
  await page.getByRole("button", { name: "Save draft" }).click()
  await expect(page.getByText("Purchase order saved as draft.")).toBeVisible()
  await expect(page.getByRole("region", { name: "Lines" })).toContainText("7.50")

  const next = page.getByRole("region", { name: "Next step" })
  await next.getByRole("button", { name: "Confirm order" }).click()
  await page.getByRole("dialog").getByRole("button", { name: "Confirm order" }).click()
  await expect(page.getByText("Order confirmed.")).toBeVisible()

  await next.getByRole("button", { name: "Receive goods" }).click()
  let dialog = page.getByRole("dialog")
  await dialog.getByLabel(/e2e bought jar/).fill("1")
  await dialog.getByRole("button", { name: "Receive goods" }).click()
  await expect(page.getByText(/Receipt GR-\d+ recorded\./)).toBeVisible()

  await next.getByRole("button", { name: "Close remainder" }).click()
  dialog = page.getByRole("dialog")
  await expect(dialog.getByLabel(/e2e bought jar/)).toHaveValue("2.00")
  await dialog.getByLabel("Reason").fill("Supplier ran out")
  await dialog.getByRole("button", { name: "Close remainder" }).click()
  await expect(page.getByText("Open quantities closed.")).toBeVisible()
  await expect(page.getByText("Received", { exact: true }).first()).toBeVisible()
  await expectNoRawValues(page, { allow })

  await page.getByRole("region", { name: "Receipts" }).getByRole("link").first().click()
  await page.getByRole("button", { name: "Book into stock" }).click()
  dialog = page.getByRole("dialog")
  await dialog.getByRole("button", { name: "Review" }).click()
  await dialog.getByRole("button", { name: "Book into stock" }).click()
  await expect(page.getByText("Booked into stock.")).toBeVisible()
  expect(await level(owner, w.product, w.warehouse)).toBe(before + 1)
})
