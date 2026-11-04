import type { Page } from "@playwright/test"

import {
  CANCEL_PAYMENT_EMAIL,
  CHECKOUT_DOWN_EMAIL,
  LATE_PAYMENT_EMAIL,
  MOCK_ORIGIN,
  APP_KEY,
  SECRET_BODY,
} from "../../mock/fixtures.mjs"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { seedCart } from "./cart"
import { placeOrder } from "./orders"

async function submit(page: Page, email: string) {
  await page.goto("/checkout")
  await page.getByLabel("Email").fill(email)
  await page.getByLabel("Full name").fill("Ada Buyer")
  await page.getByLabel("Address", { exact: true }).fill("1 Main St")
  await page.getByLabel("Postal code").fill("10001")
  await page.getByLabel("City").fill("New York")
  await page.getByLabel("Country").fill("US")
  await page.getByRole("button", { name: "Place order" }).click()
}

/** Waits for the provider's redirect back onto the order page. */
const backOn = (page: Page, id: string) =>
  page.waitForResponse(
    (r) =>
      r.request().isNavigationRequest() && r.url().endsWith(`/orders/${id}`)
  )

test("API down while placing the order: an error, the form and cart kept", async ({
  page,
}) => {
  await seedCart(page, 1)
  await submit(page, CHECKOUT_DOWN_EMAIL)
  const error = page
    .getByRole("alert")
    .filter({ hasText: "We can't place orders right now" })
  await expect(error).toHaveAttribute("data-variant", "error")
  await expect(page).toHaveURL("/checkout")
  await expect(page.getByRole("link", { name: "Cart, 1 item" })).toBeVisible()
  await expect(page.getByRole("button", { name: "Place order" })).toBeEnabled()
  expect(await page.content()).not.toContain(SECRET_BODY)
  await expectNoHorizontalScroll(page)
})

// BUG: React 19 resets a form whose `action` finishes, so every refused order (API down,
// 422, busy) wipes what the buyer typed and they must fill it all in again.
// apps/shop/components/checkout-form.tsx (`<form action={action}>`, uncontrolled inputs).
for (const [why, email] of [
  ["the API is down", CHECKOUT_DOWN_EMAIL],
  ["the API refuses a field (422)", "not-an-email"],
] as const)
  test(`what the buyer typed survives a refused order: ${why}`, async ({
    page,
  }) => {
    test.fail(true, "checkout form is reset after every refused submit")
    await seedCart(page, 1)
    await submit(page, email)
    await expect(
      page.getByRole("alert").filter({ hasText: /Please check|can't place/ })
    ).toBeVisible()
    await expect(page.getByLabel("Full name")).toHaveValue("Ada Buyer", {
      timeout: 2_000,
    })
    await expect(page.getByLabel("Email")).toHaveValue(email, {
      timeout: 2_000,
    })
  })

test("cancelling on the provider's page comes back unpaid, ready to pay again", async ({
  page,
  request,
}) => {
  const id = await placeOrder(request, CANCEL_PAYMENT_EMAIL)
  await seedCart(page, 1)
  await page.goto(`/orders/${id}`)
  const back = backOn(page, id)
  await page.getByRole("button", { name: /^Pay \$\d/ }).click()
  await back

  await expect(
    page.getByRole("status").filter({ hasText: "Waiting for payment." })
  ).toBeVisible()
  await expect(
    page.getByText("Awaiting payment", { exact: true })
  ).toBeVisible()
  await expect(page.getByRole("button", { name: /^Pay \$\d/ })).toBeEnabled()
  await expect(page.getByText(/Thank you/)).toHaveCount(0)
  // Not paid, so the cart is still there to come back to.
  await expect(page.getByRole("link", { name: "Cart, 1 item" })).toBeVisible()
})

test("a page left open after the order was paid elsewhere: Pay says it can't be paid any more", async ({
  page,
  request,
}) => {
  // Frozen clock: the page doesn't re-read the order by itself, so it stays stale.
  await page.clock.install()
  const id = await placeOrder(request, LATE_PAYMENT_EMAIL)
  await page.goto(`/orders/${id}`)
  await expect(
    page.getByRole("status").filter({ hasText: "Waiting for payment." })
  ).toBeVisible()
  // This order turns paid on its second read (here, outside the page).
  const read = await request.get(`${MOCK_ORIGIN}/v1/storefront/orders/${id}`, {
    headers: { Authorization: `Bearer ${APP_KEY}` },
  })
  expect(
    ((await read.json()) as { data: { status: string } }).data.status
  ).toBe("open")

  await page.getByRole("button", { name: /^Pay \$\d/ }).click()
  const warning = page
    .locator('[data-variant="warning"]')
    .filter({ hasText: "This order can't be paid any more" })
  await expect(warning).toContainText("Refresh the page to see its status.")
  await page.reload()
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Thank you! Your payment is in" })
  ).toBeVisible()
})

test.describe("network failures", () => {
  test.use({ allow: { failures: ["/checkout"] } })

  test("the connection drops while placing the order: an error, the form and cart kept", async ({
    page,
  }) => {
    // BUG: the failed server action throws into app/error.tsx, which replaces the whole
    // checkout with "We can't reach the catalog" (wrong words) and loses the form.
    test.fail(true, "a dropped checkout POST shows the catalogue error page")
    await seedCart(page, 1)
    await page.route("**/checkout", (route) =>
      route.request().method() === "POST"
        ? route.abort("internetdisconnected")
        : route.continue()
    )
    await submit(page, "buyer@e2e.test")
    await expect(page).toHaveURL("/checkout")
    await expect(
      page.getByRole("alert").filter({ hasText: "The shop is taking a break" })
    ).toHaveCount(0, { timeout: 2_000 })
    await expect(page.getByLabel("Email")).toHaveValue("buyer@e2e.test")
    await expect(
      page.getByRole("button", { name: "Place order" })
    ).toBeEnabled()
    await expect(page.getByRole("link", { name: "Cart, 1 item" })).toBeVisible()
  })

  test("offline while waiting for the payment: the page stays, and catches up once back", async ({
    page,
    context,
    request,
  }) => {
    // BUG: PaymentWatcher's router.refresh() fails offline and Next falls back to a full
    // browser navigation, so the order page turns into the browser's offline error page.
    // apps/shop/components/payment-watcher.tsx (setInterval → router.refresh()).
    test.fail(
      true,
      "offline, the order page navigates away to the browser error page"
    )
    await page.clock.install()
    const id = await placeOrder(request, LATE_PAYMENT_EMAIL)
    await page.goto(`/orders/${id}`)
    const status = page.getByRole("status")
    await expect(
      status.filter({ hasText: "Waiting for payment." })
    ).toBeVisible()

    await context.setOffline(true)
    await page.clock.runFor(7_000)
    await expect(
      status.filter({ hasText: "Waiting for payment." })
    ).toBeVisible()
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Order")

    await context.setOffline(false)
    await page.clock.runFor(7_000)
    await expect(
      status.filter({ hasText: "Thank you! Your payment is in" })
    ).toBeVisible()
  })
})
