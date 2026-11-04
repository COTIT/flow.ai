import { expect, test, urls } from "../../src/stack"
import { expectNoRawValues } from "../../src/test"
import { backdateHold, buy, movedFor, ownProduct, storefront } from "./shop"

test("an order not paid in time expires: Pay is refused, then the page says so and the stock is back", async ({
  page,
  request,
}) => {
  // The expiry job runs once a minute.
  test.setTimeout(180_000)
  const { owner, id } = await ownProduct(request, { stock: 5 })
  const read = storefront(request)
  const before = await read<{ available_stock: string }>(`/products/${id}`)
  const orderId = await buy(page, id, 2)
  const held = await read<{ available_stock: string }>(`/products/${id}`)
  expect(Number(held.available_stock), "two held by the order").toBe(
    Number(before.available_stock) - 2
  )

  backdateHold(orderId)
  await page.getByRole("button", { name: /^Pay \$\d/ }).click()
  await expect(
    page
      .locator('[data-variant="warning"]')
      .filter({ hasText: "This order can't be paid any more" })
  ).toBeVisible()

  await expect
    .poll(
      async () => (await read<{ status: string }>(`/orders/${orderId}`)).status,
      { timeout: 120_000 }
    )
    .toBe("cancelled")
  await page.goto(`${urls.shop}/orders/${orderId}`)
  await expect(
    page.getByText("This order expired before it was paid")
  ).toBeVisible()
  await expect(page.getByRole("button", { name: /^Pay \$\d/ })).toHaveCount(0)
  await expectNoRawValues(page)
  const after = await read<{ available_stock: string }>(`/products/${id}`)
  expect(Number(after.available_stock), "hold given back").toBe(
    Number(before.available_stock)
  )
  expect(
    await movedFor(owner, id, orderId),
    "nothing ever left the warehouse"
  ).toBe(0)
})
