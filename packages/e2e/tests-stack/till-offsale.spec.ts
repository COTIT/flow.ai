import type { APIRequestContext, BrowserContext, Page } from "@playwright/test"

import {
  expect,
  openShift,
  pairTill,
  stackAllow,
  test,
  type Api,
} from "../src/stack"
import {
  fixedRef,
  ownProduct,
  replayAllow,
  salesOf,
  tile,
  tillApi,
  toSale,
} from "./till"

const offSale = (owner: Api, id: string) =>
  owner.post(owner.ws(`/products/${id}`), { status: "disabled" })

test.describe("taken off sale while in the basket", () => {
  test.use({
    allow: {
      ...stackAllow,
      responses: [
        ...stackAllow.responses,
        { url: tillApi("/sales"), status: 422 },
        { url: tillApi("/sales/quote"), status: 422 },
      ],
    },
  })

  test("the API refuses the sale, the till says why, and the next sync drops the tile", async ({
    page,
    request,
  }) => {
    const { owner, register } = await pairTill(page, request)
    await openShift(page)
    const product = await ownProduct(owner, register, "offsale")
    await toSale(page)
    await (await tile(page, product.name)).click()

    await offSale(owner, product.id)
    await page.getByRole("button", { name: /^Charge/ }).click()
    const sheet = page.getByRole("dialog", { name: "Take payment" })
    await sheet.getByRole("button", { name: "Card" }).click()
    await sheet.getByRole("button", { name: /^Book card sale/ }).click()
    await expect(
      sheet
        .getByRole("alert")
        .filter({ hasText: "This product is not for sale." })
    ).toBeVisible()
    await expect(page.getByRole("dialog", { name: "Done" })).toHaveCount(0)
    expect(await salesOf(owner, register), "nothing booked").toHaveLength(0)

    await toSale(page)
    await page.getByLabel("Scan a barcode or type a name").fill(product.name)
    await expect(
      page.getByText(`No products match “${product.name}”.`)
    ).toBeVisible()
  })
})

test.describe("replayed after it went off sale", () => {
  test.use({
    allow: { ...stackAllow, responses: [...stackAllow.responses, replayAllow] },
  })

  /** Rings up the product offline, takes it off sale in the office, then lets the till replay. */
  async function replayAfterOffSale(
    page: Page,
    context: BrowserContext,
    request: APIRequestContext
  ) {
    await fixedRef(page)
    const { owner, register } = await pairTill(page, request)
    await openShift(page)
    const product = await ownProduct(owner, register, "late")
    await toSale(page)
    await tile(page, product.name)
    await context.setOffline(true)
    await (await tile(page, product.name)).click()
    await page.getByRole("button", { name: /^Charge/ }).click()
    const sheet = page.getByRole("dialog", { name: "Take payment" })
    await sheet.getByRole("button", { name: "Card" }).click()
    await sheet.getByRole("button", { name: /^Book card sale/ }).click()
    const saved = page.getByRole("dialog", { name: "Saved offline" })
    await expect(saved).toBeVisible()
    await saved.getByRole("button", { name: "New sale" }).click()

    await offSale(owner, product.id)
    await context.setOffline(false)
    await page.getByRole("link", { name: "Offline sales synced" }).click()
    const card = page.getByRole("listitem").filter({ hasText: "1 item" })
    await expect(card.getByText("Booked", { exact: true })).toBeVisible()
    return { card, sales: await salesOf(owner, register) }
  }

  test("an offline sale rung up before the product went off sale is still booked, and the API flags it", async ({
    page,
    context,
    request,
  }) => {
    const { sales } = await replayAfterOffSale(page, context, request)
    expect(sales).toHaveLength(1)
    expect(sales[0]!.unsellable_lines).toBe(1)
  })

  test("that replayed sale is flagged on the till for the office", async ({
    page,
    context,
    request,
  }) => {
    test.fail(
      true,
      "till ignores SaleResource.unsellable_lines: no warning on the queue card (reported by e2e-till)"
    )
    const { card } = await replayAfterOffSale(page, context, request)
    await expect(card.getByRole("status")).toBeVisible({ timeout: 2_000 })
  })
})
