import { randomUUID } from "node:crypto"

import type { Page } from "@playwright/test"

import { expect, unique, urls, workspaces, type Api } from "../src/stack"

/** The till API path in the AFM workspace, as the till app calls it (absolute, for `allow`). */
export const tillApi = (path: string) =>
  `${urls.till}/v1/workspace/${workspaces.AFM}/till${path}`

/** `GET /till/sales` row, the fields these specs read. */
export type Sale = {
  order_id: string
  number: string
  client_ref: string
  register_id: string
  method: string
  total: number
  due: number
  unsellable_lines: number
}

export type Shift = {
  opening: number
  expected: number
  counted: number | null
  difference: number | null
}

/** A product of the test's own (data rule 1), on sale at 4.00 excl. VAT, 20 in the till's store. */
export async function ownProduct(
  owner: Api,
  register: { warehouse_id: string },
  what = "till"
) {
  const name = `e2e ${what} ${unique()}`
  const { product_id: id } = await owner.post<{ product_id: string }>(
    owner.ws("/products"),
    {
      name,
      price: "4.00",
      store_only: false,
    }
  )
  await owner.post(owner.ws(`/products/${id}/set-stock-levels`), {
    warehouses: [
      { warehouse_id: register.warehouse_id, stock_lvl: 20, reason: "e2e" },
    ],
  })
  return { id, name }
}

/** The register's sales, newest first. */
export const salesOf = (owner: Api, register: { id: string }) =>
  owner.get<Sale[]>(
    owner.ws(`/till/sales?register_id=${register.id}&page_size=100`)
  )

/** The product's tile on the sale screen, found by searching its unique name. */
export async function tile(page: Page, name: string) {
  await page.getByLabel("Scan a barcode or type a name").fill(name)
  const found = page
    .getByRole("list", { name: "Products" })
    .getByRole("button", { name: new RegExp(name) })
  await expect(found).toBeVisible()
  return found
}

/** Formats minor units the way the till does, in the paired register's currency. */
export async function tillMoney(page: Page) {
  const { code, decimals } = await page.evaluate(
    () =>
      JSON.parse(localStorage.getItem("genesy.till")!).state.session.register
        .currency
  )
  const f = new Intl.NumberFormat("en", {
    style: "currency",
    currency: code,
    minimumFractionDigits: decimals,
  })
  // Integer maths into a decimal string, as the till's formatMoney: no float on the way.
  return (minor: number) => {
    const abs = String(Math.abs(minor)).padStart(decimals + 1, "0")
    const text = `${minor < 0 ? "-" : ""}${abs.slice(0, abs.length - decimals)}${decimals ? `.${abs.slice(-decimals)}` : ""}`
    return f.format(text as unknown as number)
  }
}

/** Opens the sale screen fresh, so its catalog sync picks up products made since. */
export const toSale = (page: Page) => page.goto(`${urls.till}/sale`)

/**
 * One client_ref per spec file (and worker), known before the test, so the replay's first
 * question (`GET /sales/<ref>`, 404 until booked) can be allowed exactly. Each test sells on a
 * register of its own, where the API keeps refs unique.
 */
export const REF = randomUUID()

/** Every sale this page rings up gets REF as its client_ref. */
export async function fixedRef(page: Page) {
  await page.addInitScript((r) => {
    crypto.randomUUID = () =>
      r as `${string}-${string}-${string}-${string}-${string}`
  }, REF)
}

/** What a replay may meet before its sale exists: the reconcile 404 on its own ref. */
export const replayAllow = { url: tillApi(`/sales/${REF}`), status: 404 }
