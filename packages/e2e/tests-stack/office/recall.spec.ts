import type { APIRequestContext, Page } from "@playwright/test"

import { expectNoRawValues } from "../../src/test"
import { type Api, apiAs, cred, expect, loginOffice, stackAllow, test, unique, urls, workspaces } from "../../src/stack"

type Lot = { id: string; code: string; status: string }
type LotStock = { warehouse_id: string; level: string; reserved: string; available: string }
type Order = { id: string; number: string; status: string; warehouse_id: string }

const owner = (request: APIRequestContext) => apiAs(request, cred("office (AFM)", "owner"))

/**
 * A stock-tracked product, a draft order of 3 for it (its warehouse is where lots get stock),
 * and lots of it with stock there: `[code, expires_on, quantity]`, earliest expiry held first.
 */
async function world(api: Api, lots: [string, string, string][]) {
  const tag = unique()
  const { product_id: product } = await api.post<{ product_id: string }>(api.ws("/products"), {
    name: `e2e recall jar ${tag}`,
    price: "5.00",
    has_stock: true,
  })
  const { relation_id } = await api.post<{ relation_id: string }>(api.ws("/relations"), {
    type: "business",
    company_name: `e2e recall customer ${tag}`,
  })
  const { order_id } = await api.post<{ order_id: string }>(api.ws("/orders"), {
    relation_id,
    lines: [{ product_id: product, quantity: "3" }],
  })
  const order = await api.get<Order>(api.ws(`/orders/${order_id}`))
  const made: Lot[] = []
  for (const [code, expires_on, quantity] of lots) {
    const lot = await api.post<Lot>(api.ws("/lots"), {
      product_id: product,
      code: `${code}-${tag}`.toUpperCase(),
      expires_on,
    })
    await api.post(api.ws(`/lots/${lot.id}/stock-mutations`), { warehouse_id: order.warehouse_id, quantity })
    made.push(lot)
  }
  await api.post(api.ws(`/orders/${order.id}/confirm`))
  return { order, lots: made }
}

/** Recalls a batch from its office page and waits for the new status. */
async function recall(page: Page, lotId: string) {
  await page.goto(`${urls.office}/lots/${lotId}`)
  await page.getByRole("button", { name: "Recall this batch…" }).click()
  await page.getByRole("alertdialog").getByRole("button", { name: "Recall", exact: true }).click()
  await expect(page.getByRole("region", { name: "Status" }).getByText("Recalled", { exact: true })).toBeVisible()
}

/** Records picking and completes the order from its office page. */
async function pickAndComplete(page: Page, orderId: string) {
  await page.goto(`${urls.office}/orders/${orderId}`)
  await page.getByRole("button", { name: "Record picking" }).click()
  await page.getByRole("dialog").getByRole("button", { name: "Record picking" }).click()
  await expect(page.getByRole("dialog")).toBeHidden()
  await page.getByRole("main").getByRole("button", { name: "Complete" }).click()
  await page.getByRole("alertdialog").getByRole("button", { name: "Complete" }).click()
}

test("recall: the order's hold moves to the next batch and the order still ships", async ({ page, request }) => {
  const api = await owner(request)
  const { order, lots } = await world(api, [
    ["BAD", "2098-01-01", "5"],
    ["GOOD", "2099-01-01", "5"],
  ])
  const [bad, good] = lots as [Lot, Lot]
  expect((await api.get<LotStock[]>(api.ws(`/lots/${bad.id}/stock`)))[0]?.reserved).toBe("3.00")

  await loginOffice(page, cred("office (AFM)", "owner"))
  await recall(page, bad.id)
  // Nothing waits on the recalled batch: the hold left it whole.
  await expect(page.getByRole("region", { name: "Orders waiting on this batch" })).toHaveCount(0)
  expect((await api.get<LotStock[]>(api.ws(`/lots/${bad.id}/stock`)))[0]?.reserved).toBe("0.00")
  expect((await api.get<LotStock[]>(api.ws(`/lots/${good.id}/stock`)))[0]?.reserved).toBe("3.00")

  await pickAndComplete(page, order.id)
  await expect(page.getByRole("status").filter({ hasText: "Order completed; stock taken." })).toBeVisible()
  expect((await api.get<LotStock[]>(api.ws(`/lots/${good.id}/stock`)))[0]?.level).toBe("2.00")
  expect((await api.get<LotStock[]>(api.ws(`/lots/${bad.id}/stock`)))[0]?.level).toBe("5.00")
})

test.describe(() => {
  // Made before the test so the allowed 409 can name the order (allow is read as the test starts).
  let w: Awaited<ReturnType<typeof world>>
  test.beforeAll(async ({ request }) => {
    w = await world(await owner(request), [["ONLY", "2099-01-01", "5"]])
  })
  test.use({
    // eslint-disable-next-line no-empty-pattern -- Playwright reads fixture deps from the pattern
    allow: async ({}, use) =>
      use({
        ...stackAllow,
        responses: [
          ...stackAllow.responses,
          { url: `${urls.office}/v1/workspace/${workspaces.AFM}/orders/${w.order.id}/complete`, status: 409 },
        ],
      }),
  })

  test("recall with no other batch: the batch lists the order, which can't ship and says why", async ({
    page,
    request,
  }) => {
    const api = await owner(request)
    const [only] = w.lots as [Lot]
    await loginOffice(page, cred("office (AFM)", "owner"))
    await recall(page, only.id)

    const waiting = page.getByRole("region", { name: "Orders waiting on this batch" })
    await expect(waiting.getByText("3.00 held")).toBeVisible()
    await expect(waiting.getByRole("link", { name: w.order.number })).toHaveAttribute("href", `/orders/${w.order.id}`)
    await expectNoRawValues(page, { allow: [only.code, w.order.number] })

    await pickAndComplete(page, w.order.id)
    await expect(
      page.getByRole("alert").filter({ hasText: "A batch this order holds was recalled" })
    ).toBeVisible()
    await expect(page.getByRole("main")).not.toContainText("lot_recalled")
    expect((await api.get<Order>(api.ws(`/orders/${w.order.id}`))).status).toBe("open")
    // The hold is untouched until the office cancels the order.
    expect((await api.get<LotStock[]>(api.ws(`/lots/${only.id}/stock`)))[0]?.reserved).toBe("3.00")
    await api.post(api.ws(`/orders/${w.order.id}/cancel`))
  })
})

test.describe(() => {
  let lot: Lot
  test.beforeAll(async ({ request }) => {
    const api = await owner(request)
    const { lots } = await world(api, [["STOCKIN", "2099-01-01", "5"]])
    lot = lots[0]!
    await api.patch(api.ws(`/lots/${lot.id}`), { status: "recalled" })
  })
  test.use({
    // eslint-disable-next-line no-empty-pattern -- Playwright reads fixture deps from the pattern
    allow: async ({}, use) =>
      use({
        ...stackAllow,
        responses: [
          ...stackAllow.responses,
          { url: `${urls.office}/v1/workspace/${workspaces.AFM}/lots/${lot.id}/stock-mutations`, status: 409 },
        ],
      }),
  })

  test("a recalled batch takes no new stock: the office says so in words", async ({ page, request }) => {
    const api = await owner(request)
    await loginOffice(page, cred("office (AFM)", "owner"))
    await page.goto(`${urls.office}/lots/${lot.id}`)
    // Recalled is final: the status picker locks.
    await expect(page.getByRole("combobox", { name: "Status" })).toBeDisabled()
    await page.getByRole("button", { name: "Change stock" }).click()
    const dialog = page.getByRole("dialog")
    await expect(dialog.getByLabel("Warehouse")).not.toHaveValue("")
    await dialog.getByLabel("Quantity").fill("1")
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(
      dialog.getByRole("alert").filter({ hasText: "This batch is recalled, so no stock can be added." })
    ).toBeVisible()
    await expect(dialog).not.toContainText("lot_recalled")
    expect((await api.get<LotStock[]>(api.ws(`/lots/${lot.id}/stock`)))[0]?.level).toBe("5.00")
  })
})

test("a warehouse holding stock is not deleted", async ({ request }) => {
  const api = await owner(request)
  const { order } = await world(api, [["HELD", "2099-01-01", "5"]])
  const del = await request.fetch(`${urls.api}/v1${api.ws(`/warehouses/${order.warehouse_id}`)}`, {
    method: "DELETE",
    headers: { Authorization: `Bearer ${api.token}` },
  })
  expect(del.status()).toBe(409)
  expect(await del.json()).toEqual({ message: "warehouse_holds_stock" })
  await api.get(api.ws(`/warehouses/${order.warehouse_id}`))

  await api.post(api.ws(`/orders/${order.id}/cancel`))
})
