import { LATE_PAYMENT_EMAIL } from "../../mock/fixtures.mjs"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { placeOrder } from "./orders"

test("an unpaid order page picks up the payment by itself, without a reload", async ({
  page,
  request,
}) => {
  // The mock marks this order paid on its second read: the buyer is back from the
  // provider before its webhook has landed.
  await page.goto(`/orders/${await placeOrder(request, LATE_PAYMENT_EMAIL)}`)
  const status = page.getByRole("status")
  await expect(status.filter({ hasText: "Waiting for payment." })).toBeVisible()
  await expect(page.getByRole("button", { name: /^Pay \$\d/ })).toBeVisible()
  await expectNoHorizontalScroll(page)
  // Survives a client-side refresh, not a full page load.
  await page.evaluate(() => Object.assign(window, { stillHere: true }))

  // The page re-reads the order every 3 s; wait for the announced paid state.
  await expect(
    status.filter({ hasText: "Thank you! Your payment is in" })
  ).toBeVisible({ timeout: 10_000 })
  await expect(page.getByRole("button", { name: /^Pay \$\d/ })).toHaveCount(0)
  expect(await page.evaluate(() => "stillHere" in window)).toBe(true)
})
