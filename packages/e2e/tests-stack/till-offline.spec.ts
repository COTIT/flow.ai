import type { Page } from "@playwright/test"

import {
  expect,
  openShift,
  pairTill,
  stackAllow,
  test,
  urls,
} from "../src/stack"
import {
  fixedRef,
  ownProduct,
  REF,
  replayAllow,
  salesOf,
  tile,
  tillApi,
  toSale,
} from "./till"

/**
 * Rings up one of `name` and books it as cash: 10.00 typed on the pad offline (minor units; no
 * quote to offer Exact), Exact online.
 */
async function sellOne(page: Page, name: string, offline: boolean) {
  await (await tile(page, name)).click()
  await page.getByRole("button", { name: /^Charge/ }).click()
  const sheet = page.getByRole("dialog", { name: "Take payment" })
  await sheet.getByRole("button", { name: "Cash" }).click()
  if (offline)
    for (const d of "1000")
      await sheet.getByRole("button", { name: d, exact: true }).click()
  else await sheet.getByRole("button", { name: "Exact" }).click()
  await sheet.getByRole("button", { name: /^Book sale/ }).click()
}

test.use({
  allow: { ...stackAllow, responses: [...stackAllow.responses, replayAllow] },
})

test("a sale rung up offline is booked once when the till is back, and never again", async ({
  page,
  context,
  request,
}) => {
  await fixedRef(page)
  const { owner, register } = await pairTill(page, request)
  await openShift(page)
  const product = await ownProduct(owner, register, "offline")
  await toSale(page)
  await tile(page, product.name)

  await context.setOffline(true)
  await sellOne(page, product.name, true)
  const saved = page.getByRole("dialog", { name: "Saved offline" })
  await expect(saved).toBeVisible()
  await saved.getByRole("button", { name: "New sale" }).click()
  expect(
    await salesOf(owner, register),
    "nothing booked while offline"
  ).toHaveLength(0)

  await context.setOffline(false)
  await page.getByRole("link", { name: "Offline sales synced" }).click()
  const card = page.getByRole("listitem").filter({ hasText: "1 item" })
  await expect(card.getByText("Booked", { exact: true })).toBeVisible()
  const sales = await salesOf(owner, register)
  expect(sales.map((s) => s.client_ref)).toEqual([REF])
  await expect(
    card.getByText(`Booked as sale ${sales[0]!.number}`)
  ).toBeVisible()

  // Sync now and a reload find nothing left to send: still one sale.
  await page.getByRole("button", { name: "Sync now" }).click()
  await page.reload()
  await expect(card.getByText("Booked", { exact: true })).toBeVisible()
  expect(await salesOf(owner, register)).toHaveLength(1)
})

test.describe("reply lost mid-payment", () => {
  test.use({
    allow: {
      ...stackAllow,
      responses: [...stackAllow.responses, replayAllow],
      failures: [tillApi("/sales")],
    },
  })

  test("the server booked it, the till never heard: the replay finds it, no second sale", async ({
    page,
    request,
  }) => {
    const { owner, register } = await pairTill(page, request)
    await openShift(page)
    const product = await ownProduct(owner, register, "lost")
    // The sale reaches the API, then the connection drops before the till gets the answer.
    await page.route(tillApi("/sales"), async (route) => {
      if (route.request().method() !== "POST") return route.fallback()
      await route.fetch()
      await route.abort("connectionreset")
    })
    await toSale(page)
    await sellOne(page, product.name, false)

    await expect(
      page.getByRole("dialog", { name: "Saved offline" })
    ).toBeVisible()
    await page.goto(`${urls.till}/queue`)
    await expect(page.getByText("Booked", { exact: true })).toBeVisible()
    const sales = await salesOf(owner, register)
    expect(sales, "one sale for the one payment").toHaveLength(1)
    await expect(
      page.getByText(`Booked as sale ${sales[0]!.number}`)
    ).toBeVisible()
  })
})
