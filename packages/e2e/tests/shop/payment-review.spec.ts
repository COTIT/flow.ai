import {
  APP_KEY,
  MOCK_ORIGIN,
  UNKNOWN_PAYMENT_EMAIL,
  EXISTING_UNKNOWN_PAYMENT_EMAIL,
  PAYMENT_SETUP_REQUIRED_EMAIL,
  STALE_PAYMENT_SETUP_EMAIL,
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
  await expect(
    page.getByText("Online payment is currently unavailable", { exact: true })
  ).toHaveCount(0)
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

for (const [locale, title, pay] of [
  ["en", "Online payment is currently unavailable", /^Pay /],
  ["nl", "Online betalen is momenteel niet beschikbaar", /^Betaal /],
  ["sw", "Malipo ya mtandaoni hayapatikani kwa sasa", /^Lipa /],
] as const)
  test(`${locale} setup unavailable disables payment before initial render and after reload`, async ({
    page,
    request,
    baseURL,
  }) => {
    const id = await placeOrder(request, PAYMENT_SETUP_REQUIRED_EMAIL)
    let paymentPosts = 0
    page.on("request", (call) => {
      if (
        call.method() === "POST" &&
        new URL(call.url()).pathname === `/orders/${id}`
      )
        paymentPosts++
    })
    await page
      .context()
      .addCookies([{ name: "NEXT_LOCALE", value: locale, url: baseURL! }])
    await page.clock.install()
    await page.goto(`/orders/${id}`)
    await expect(page.getByText(title, { exact: true })).toBeVisible()
    await expect(page.getByRole("button", { name: pay })).toBeDisabled()
    await page.clock.fastForward(9000)
    await page.reload()
    await expect(page.getByText(title, { exact: true })).toBeVisible()
    await expect(page.getByRole("button", { name: pay })).toBeDisabled()
    const order = await request.get(
      `${MOCK_ORIGIN}/v1/storefront/orders/${id}`,
      { headers: { Authorization: `Bearer ${APP_KEY}` } }
    )
    expect((await order.json()).data.payment_attempts).toBe(0)
    expect(paymentPosts).toBe(0)
    await expectNoHorizontalScroll(page)
  })

test("a stale payment button handles the stable setup code once and remains disabled after reload", async ({
  page,
  request,
}) => {
  const id = await placeOrder(request, STALE_PAYMENT_SETUP_EMAIL)
  await page.clock.install()
  await page.goto(`/orders/${id}`)
  await page.getByRole("button", { name: /^Pay \$/ }).click()
  await expect(
    page.getByText("Online payment is currently unavailable", { exact: true })
  ).toBeVisible()
  await expect(page.getByRole("button", { name: /^Pay \$/ })).toBeDisabled()
  await page.reload()
  await expect(
    page.getByText("Online payment is currently unavailable", { exact: true })
  ).toBeVisible()
  await expect(page.getByRole("button", { name: /^Pay \$/ })).toBeDisabled()
  const order = await request.get(`${MOCK_ORIGIN}/v1/storefront/orders/${id}`, {
    headers: { Authorization: `Bearer ${APP_KEY}` },
  })
  expect((await order.json()).data.payment_attempts).toBe(1)
  await expect(
    page.getByText(
      /secret upstream detail|credentials|configuration|verification/i
    )
  ).toHaveCount(0)
  await expectNoHorizontalScroll(page)
})
