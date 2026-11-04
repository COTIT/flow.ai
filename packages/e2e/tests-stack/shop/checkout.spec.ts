import { cred, expect, loginOffice, notes, test, urls } from "../../src/stack"
import { expectNoRawValues } from "../../src/test"
import {
  buy,
  lastPayment,
  movedFor,
  ownProduct,
  payOnFakePage,
  storefront,
} from "./shop"

type Order = {
  id: string
  number: string
  status: string
  total: number
  currency: string
  decimals: number
}

const money = (o: Pick<Order, "currency" | "decimals">, minor: number) =>
  new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: o.currency,
  }).format(minor / 10 ** o.decimals)

test("buy on the shop, pay on the provider page, the office completes it and only then is stock taken", async ({
  page,
  request,
}) => {
  test.setTimeout(120_000)
  const { owner, id } = await ownProduct(request)
  const read = storefront(request)

  const orderId = await buy(page, id, 2)
  const placed = await read<Order>(`/orders/${orderId}`)
  expect(placed.status).toBe("awaiting_payment")
  await expect(page.getByRole("heading", { level: 1 })).toContainText(
    placed.number
  )
  await expect(
    page.getByRole("button", { name: `Pay ${money(placed, placed.total)}` })
  ).toBeVisible()

  await payOnFakePage(page, "Pay")
  // The provider's answer lands as a job; the order page re-reads by itself.
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Thank you! Your payment is in" })
  ).toBeVisible({ timeout: 30_000 })
  await expect(page.getByRole("link", { name: "Cart, empty" })).toBeVisible()
  await expectNoRawValues(page)
  expect((await read<Order>(`/orders/${orderId}`)).status).toBe("open")
  // Paid is not shipped: nothing has left the warehouse yet.
  expect(
    await movedFor(owner, id, orderId),
    "stock moved before complete"
  ).toBe(0)

  await loginOffice(page, cred("office (AFM)", "owner"))
  await page.goto(`${urls.office}/orders/${orderId}`)
  await expect(page.getByRole("heading", { level: 1 })).toContainText(
    placed.number
  )
  // Complete needs every line picked; the dialog starts at the full quantity.
  await page.getByRole("button", { name: "Record picking" }).click()
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Record picking" })
    .click()
  await expect(page.getByText("Picking saved.")).toBeVisible()
  await page.getByRole("button", { name: "Complete", exact: true }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Complete", exact: true })
    .click()
  await expect(page.getByText("Order completed; stock taken.")).toBeVisible()
  await expectNoRawValues(page)
  expect(await movedFor(owner, id, orderId), "stock taken by complete").toBe(-2)

  // The buyer's page still reads as paid.
  await page.goto(`${urls.shop}/orders/${orderId}`)
  await expect(page.getByText("Paid", { exact: true })).toBeVisible()
})

test("cancel on the provider page: back unpaid, the stock still held, and Pay works again", async ({
  page,
  request,
}) => {
  test.setTimeout(120_000)
  const { id } = await ownProduct(request)
  const read = storefront(request)
  const orderId = await buy(page, id)

  await payOnFakePage(page, "Cancel")
  await expect(
    page.getByRole("status").filter({ hasText: "Waiting for payment." })
  ).toBeVisible()
  await expect(
    page.getByText("Awaiting payment", { exact: true })
  ).toBeVisible()
  expect((await read<Order>(`/orders/${orderId}`)).status).toBe(
    "awaiting_payment"
  )
  await expect(page.getByRole("link", { name: "Cart, 1 item" })).toBeVisible()
  await expectNoRawValues(page)

  // Paying again before the API has recorded the cancel reopens the cancelled payment (a dead
  // end, reported); wait for the cancel to land, as a buyer reading the page would.
  await expect
    .poll(() => lastPayment(orderId), { timeout: 30_000 })
    .toBe("canceled")
  await payOnFakePage(page, "Pay")
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Thank you! Your payment is in" })
  ).toBeVisible({ timeout: 30_000 })
})

test("more than is in stock: the API's refusal shows next to the product, no order placed", async ({
  page,
  request,
}) => {
  const { name, id } = await ownProduct(request, { stock: 2 })
  await page.goto(`${urls.shop}/products/${id}`)
  for (let i = 0; i < 2; i++)
    await page.getByRole("button", { name: `One more ${name}` }).click()
  await page.getByRole("button", { name: "Add to cart" }).click()
  await page.goto(`${urls.shop}/checkout`)
  await page.getByLabel("Email").fill("buyer@e2e.test")
  await page.getByLabel("Full name").fill("Ada Buyer")
  await page.getByLabel("Address", { exact: true }).fill("1 Main St")
  await page.getByLabel("Postal code").fill("10001")
  await page.getByLabel("City").fill("New York")
  await page.getByLabel("Country").fill("US")
  await page.getByRole("button", { name: "Place order" }).click()
  await expect(
    page.getByRole("alert").filter({ hasText: "Please check your order" })
  ).toContainText(name)
  await expect(page).toHaveURL(`${urls.shop}/checkout`)
})

test("a product with no stock is sold out on the shop and can't be added", async ({
  page,
  request,
}) => {
  const { id, name } = await ownProduct(request, { stock: 0 })
  await page.goto(`${urls.shop}/products/${id}`)
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(name)
  await expect(page.getByText("Sold out for now")).toBeVisible()
  await expectNoRawValues(page)
  await expect(page.getByRole("button", { name: "Add to cart" })).toBeDisabled()
})

test("a refunded and cancelled order tells the buyer it was cancelled, not that it expired unpaid", async ({
  page,
}) => {
  // The seed's own paid, refunded and cancelled web order (read only).
  await page.goto(`${urls.shop}${notes["shop order (refunded, cancelled)"]}`)
  await expect(page.getByText("Cancelled", { exact: true })).toBeVisible()
  const status = page.getByRole("status")
  await expect(status).toContainText("This order was cancelled")
  await expect(status).toContainText("Your payment has been refunded")
  await expectNoRawValues(page)
  await expect(
    page.getByText("This order expired before it was paid")
  ).toHaveCount(0)
})
