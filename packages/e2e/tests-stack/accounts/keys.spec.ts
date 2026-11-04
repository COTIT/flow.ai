import type { APIRequestContext, Page } from "@playwright/test"

import {
  apiAs,
  cred,
  expect,
  loginOffice,
  notes,
  openShift,
  pairTill,
  test,
  unique,
  urls,
  type Api,
} from "../../src/stack"
import { test as refusing } from "./refusals"

type Key = { id: string; name: string; kind: string; revoked_at: string | null }

const disconnected =
  "This till was disconnected or paired elsewhere. Pair it again with a new code from the office."

/** Revokes the live app key called `name` (a register's device key carries its name). */
async function revokeKey(request: APIRequestContext, owner: Api, name: string) {
  const keys = await owner.get<Key[]>(owner.ws("/app-keys?page_size=100"))
  const key = keys.find((k) => k.name === name && !k.revoked_at)
  expect(key, `live key ${name}`).toBeTruthy()
  const r = await request.delete(
    `${urls.api}/v1${owner.ws(`/app-keys/${key!.id}`)}`,
    {
      headers: { Authorization: `Bearer ${owner.token}` },
    }
  )
  expect(r.status()).toBe(204)
}

/** Pairs the till again to `register` with a fresh office code, then the cashier's PIN. */
async function pairAgain(page: Page, owner: Api, registerId: string) {
  const pairing = await owner.post<{ code: string; workspace: string }>(
    owner.ws(`/till/registers/${registerId}/pairing`)
  )
  await expect(page).toHaveURL(`${urls.till}/setup`)
  await page.getByLabel("Account").fill(pairing.workspace)
  await page.getByLabel("Pairing code").fill(pairing.code)
  await page.getByRole("button", { name: "Pair this till" }).click()
  const cashier = cred("till (AFM)", "cashier PIN")
  await page.getByRole("button", { name: cashier.login }).click()
  for (const digit of cashier.secret)
    await page.getByRole("button", { name: digit, exact: true }).click()
  await page.getByRole("button", { name: "Log in" }).click()
  await expect(page).not.toHaveURL(`${urls.till}/login`)
}

// Once the key is revoked every till call answers 401 (and the queued sale's lookup 404).
refusing.describe("a revoked till key", () => {
  refusing.use({
    refused: {
      prefix: `${urls.till}/v1/workspace/`,
      statuses: [401, 404],
      offline: true,
    },
  })

  refusing(
    "a revoked till key sends the till back to pairing, and a new code brings it back",
    async ({ page, request }) => {
      const { owner, register } = await pairTill(page, request)
      await revokeKey(request, owner, register.name)

      await page.reload()
      await expect(
        page.getByRole("alert").filter({ hasText: disconnected })
      ).toBeVisible()
      await expect(page.getByText(/\b401\b/)).toHaveCount(0)
      await page.getByRole("button", { name: "Pair again" }).click()
      await pairAgain(page, owner, register.id)
      await expect(page).toHaveURL(`${urls.till}/sale`)
    }
  )

  refusing(
    "a sale queued offline survives its key being revoked and books after pairing again",
    async ({ page, context, request }) => {
      const { owner, register } = await pairTill(page, request)
      await openShift(page)
      // A product of the test's own, stocked in the till's store (data rule 1).
      const name = `e2e queued ${unique()}`
      const { product_id: id } = await owner.post<{ product_id: string }>(
        owner.ws("/products"),
        {
          name,
          price: "2.00",
        }
      )
      await owner.post(owner.ws(`/products/${id}/set-stock-levels`), {
        warehouses: [
          { warehouse_id: register.warehouse_id, stock_lvl: 5, reason: "e2e" },
        ],
      })

      await page.goto(`${urls.till}/sale`)
      await page.getByLabel("Scan a barcode or type a name").fill(name)
      await page
        .getByRole("list", { name: "Products" })
        .getByRole("button", { name: new RegExp(name) })
        .click()
      await context.setOffline(true)
      await page.getByRole("button", { name: /^Charge/ }).click()
      const sheet = page.getByRole("dialog", { name: "Take payment" })
      await sheet.getByRole("button", { name: "Cash" }).click()
      for (const d of "500")
        await sheet.getByRole("button", { name: d, exact: true }).click()
      await sheet.getByRole("button", { name: /^Book sale/ }).click()
      await expect(
        page.getByRole("dialog", { name: "Saved offline" })
      ).toBeVisible()
      await expect(page.getByText("Offline · 1 to sync")).toBeVisible()

      // The office revokes the key while the sale waits on the device.
      await revokeKey(request, owner, register.name)
      await context.setOffline(false)
      await expect(
        page.getByRole("alert").filter({ hasText: disconnected })
      ).toBeVisible()
      const queued = await page.evaluate(() =>
        JSON.stringify({ ...localStorage })
      )
      expect(queued, "the sale is still on the device").toContain(name)

      await page.getByRole("button", { name: "Pair again" }).click()
      await pairAgain(page, owner, register.id)
      await expect(page.getByText("Offline sales synced")).toBeVisible()
      const movements = await owner.get<{ quantity_change: string }[]>(
        owner.ws(`/products/${id}/stock-movements?page_size=100`)
      )
      expect(
        movements.filter((m) => m.quantity_change === "-1.00"),
        "the queued sale took its stock once"
      ).toHaveLength(1)
    }
  )
})

test("a revoked shop key is refused by the storefront API; a live one is served", async ({
  request,
}) => {
  const owner = await apiAs(request, cred("office (AFM)", "owner"))
  // A key of the test's own: revoking the shop's seeded key would close the shop for every spec.
  const made = await owner.post<{ id: string; key: string }>(
    owner.ws("/app-keys"),
    {
      name: `e2e shop key ${unique()}`,
      kind: "shop",
      scopes: ["catalog:read"],
    }
  )
  const asKey = (path: string) =>
    request.get(`${urls.api}/v1${path}`, {
      headers: { Authorization: `Bearer ${made.key}` },
    })

  expect((await asKey("/storefront/products?page_size=1")).status()).toBe(200)
  // A scope it was not given.
  expect(
    (
      await asKey("/storefront/orders/00000000-0000-4000-8000-000000000000")
    ).status()
  ).toBe(403)
  const r = await request.delete(
    `${urls.api}/v1${owner.ws(`/app-keys/${made.id}`)}`,
    {
      headers: { Authorization: `Bearer ${owner.token}` },
    }
  )
  expect(r.status()).toBe(204)
  expect((await asKey("/storefront/products?page_size=1")).status()).toBe(401)
  // The shop's own key is untouched and still serves the catalog.
  const shop = await request.get(
    `${urls.api}/v1/storefront/products?page_size=1`,
    {
      headers: {
        Authorization: `Bearer ${notes["storefront app key (shop container)"]}`,
      },
    }
  )
  expect(shop.status()).toBe(200)
})

test("app keys list their scopes and kinds as words, not codes", async ({
  page,
}) => {
  test.fail(
    true,
    "app keys show raw scope codes (catalog:read, till:login) and kinds (till, shop) (e2e-accounts B6)"
  )
  await loginOffice(page, cred("office (AFM)", "owner"))
  await page.goto(`${urls.office}/settings/app-keys`)
  // The seeded web shop key (catalog:read, checkout:write) is on the first page.
  await expect(
    page.getByText("Web shop", { exact: true }).first()
  ).toBeVisible()
  await expect(
    page.getByText(/catalog:read|checkout:write|till:login/)
  ).toHaveCount(0)
})
