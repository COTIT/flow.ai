import { execFileSync } from "node:child_process"

import type { APIRequestContext, Page } from "@playwright/test"

import {
  type Api,
  apiAs,
  container,
  cred,
  expect,
  notes,
  unique,
  urls,
} from "../../src/stack"

type Warehouse = { id: string }
type Movement = { quantity_change: string; related_id: string | null }

/** The storefront's own view, through the shop's app key (what the shop's server reads). */
export function storefront(request: APIRequestContext) {
  const headers = {
    Authorization: `Bearer ${notes["storefront app key (shop container)"]}`,
  }
  return async <T>(path: string) => {
    const r = await request.get(`${urls.api}/v1/storefront${path}`, { headers })
    expect(r.status(), `GET /storefront${path}`).toBe(200)
    return ((await r.json()) as { data: T }).data
  }
}

/**
 * A web product of the test's own (data rule 1): `price` excl VAT, `stock` units in every
 * warehouse (checkout reserves from the default one). Returns its id and name.
 */
export async function ownProduct(
  request: APIRequestContext,
  { price = "4.00", stock = 20, name = `e2e shop ${unique()}` } = {}
) {
  const owner = await apiAs(request, cred("office (AFM)", "owner"))
  const { product_id: id } = await owner.post<{ product_id: string }>(
    owner.ws("/products"),
    {
      name,
      price,
      store_only: false,
    }
  )
  const warehouses = await owner.get<Warehouse[]>(owner.ws("/warehouses"))
  await owner.post(owner.ws(`/products/${id}/set-stock-levels`), {
    warehouses: warehouses.map((w) => ({
      warehouse_id: w.id,
      stock_lvl: stock,
      reason: "e2e",
    })),
  })
  return { owner, id, name }
}

/** Stock moved for one order on one product: the sum of its movements (data rule 2). */
export async function movedFor(owner: Api, productId: string, orderId: string) {
  const all = await owner.get<Movement[]>(
    owner.ws(`/products/${productId}/stock-movements?page_size=100`)
  )
  return all
    .filter((m) => m.related_id === orderId)
    .reduce((n, m) => n + Number(m.quantity_change), 0)
}

/** From the shop's product page: `quantity` of it into the cart, then checkout placed. Returns the order id. */
export async function buy(
  page: Page,
  productId: string,
  quantity = 1,
  email = "buyer@e2e.test"
) {
  await page.goto(`${urls.shop}/products/${productId}`)
  for (let i = 1; i < quantity; i++)
    await page.getByRole("button", { name: /^One more / }).click()
  await page.getByRole("button", { name: "Add to cart" }).click()
  await page.getByRole("link", { name: "View cart" }).click()
  await page.getByRole("link", { name: "Go to checkout" }).click()
  await page.getByLabel("Email").fill(email)
  await page.getByLabel("Full name").fill("Ada Buyer")
  await page.getByLabel("Address", { exact: true }).fill("1 Main St")
  await page.getByLabel("Postal code").fill("10001")
  await page.getByLabel("City").fill("New York")
  await page.getByLabel("Country").fill("US")
  await page.getByRole("button", { name: "Place order" }).click()
  await page.waitForURL(/\/orders\/[0-9a-f-]{36}$/)
  return page.url().split("/").pop()!
}

/** Shop order page → Pay → the API's fake provider page → `choice` → back on the order page. */
export async function payOnFakePage(page: Page, choice: "Pay" | "Cancel") {
  const order = page.url()
  await page.getByRole("button", { name: /^Pay \$\d/ }).click()
  await expect(
    page.getByRole("heading", { name: "Test payment" })
  ).toBeVisible()
  await page.getByRole("button", { name: choice, exact: true }).click()
  await page.waitForURL(order)
}

/**
 * One statement in this stack's own Postgres (database `genesy`), for the test's own rows only
 * (data rule 1), where the API has no way: moving a hold into the past, reading a payment's state.
 */
export function sql(statement: string) {
  return execFileSync(
    "docker",
    [
      "exec",
      container,
      "psql",
      "-U",
      "postgres",
      "-d",
      "genesy",
      "-Atc",
      statement,
    ],
    {
      encoding: "utf8",
    }
  ).trim()
}

const uuid = (id: string) => {
  expect(id).toMatch(/^[0-9a-f-]{36}$/)
  return id
}

/** Moves one order's 30-minute hold into the past (the expiry job runs once a minute). */
export const backdateHold = (orderId: string) =>
  sql(
    `UPDATE orders SET expires_at = now() - interval '1 minute' WHERE id = '${uuid(orderId)}'`
  )

/** The state of the order's latest payment as the API has recorded it. */
export const lastPayment = (orderId: string) =>
  sql(
    `SELECT status FROM order_payments WHERE order_id = '${uuid(orderId)}' AND kind = 'payment' ORDER BY created_at DESC LIMIT 1`
  )
