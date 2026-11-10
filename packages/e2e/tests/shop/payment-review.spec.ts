import {
  APP_KEY,
  MOCK_ORIGIN,
  UNKNOWN_PAYMENT_EMAIL,
  EXISTING_UNKNOWN_PAYMENT_EMAIL,
} from "../../mock/fixtures.mjs"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { placeOrder } from "./orders"

test("unknown payment waits for confirmation without a second payment POST", async ({
  page,
  request,
}) => {
  const id = await placeOrder(request, UNKNOWN_PAYMENT_EMAIL)
  await page.clock.install()
  await page.goto(`/orders/${id}`)
  await page.getByRole("button", { name: /^Pay \$/ }).click()
  await expect(page.getByText("Payment pending review")).toBeVisible()
  await expect(page.getByRole("button", { name: /^Pay \$/ })).toHaveCount(0)
  await page.clock.fastForward(9000)
  await page.reload()
  await expect(page.getByText("Payment pending review")).toBeVisible()
  const order = await request.get(`${MOCK_ORIGIN}/v1/storefront/orders/${id}`, {
    headers: { Authorization: `Bearer ${APP_KEY}` },
  })
  expect((await order.json()).data.payment_attempts).toBe(1)
  await expectNoHorizontalScroll(page)
})

test("existing unknown payment survives reload and polling without a payment POST", async ({
  page,
  request,
}) => {
  const id = await placeOrder(request, EXISTING_UNKNOWN_PAYMENT_EMAIL)
  await page.clock.install()
  await page.goto(`/orders/${id}`)
  await expect(page.getByText("Payment pending review")).toBeVisible()
  await expect(page.getByRole("button", { name: /^Pay \$/ })).toHaveCount(0)
  await page.reload()
  await page.clock.fastForward(9000)
  await expect(page.getByText("Payment pending review")).toBeVisible()
  await expect(page.getByRole("button", { name: /^Pay \$/ })).toHaveCount(0)
  const order = await request.get(`${MOCK_ORIGIN}/v1/storefront/orders/${id}`, {
    headers: { Authorization: `Bearer ${APP_KEY}` },
  })
  expect((await order.json()).data.payment_attempts).toBe(0)
  await expectNoHorizontalScroll(page)
})
