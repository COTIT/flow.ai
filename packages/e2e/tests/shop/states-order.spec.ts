import {
  EXPIRED_EMAIL,
  FAILING_ORDER,
  PROVIDER_DOWN_EMAIL,
  REFUNDED_EMAIL,
} from "../../mock/fixtures.mjs"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { placeOrder } from "./orders"

const notice = (
  page: import("@playwright/test").Page,
  variant: string,
  title: string | RegExp
) => page.locator(`[data-variant="${variant}"]`).filter({ hasText: title })

test("awaiting payment is an info notice with the hold time", async ({
  page,
  request,
}) => {
  await page.goto(`/orders/${await placeOrder(request)}`)
  const info = notice(page, "info", "Waiting for payment.")
  await expect(info).toBeVisible()
  await expect(info).toContainText(/Your items are reserved until \d/)
  // The order's status area is one live region, so changes are announced.
  await expect(page.getByRole("status").filter({ has: info })).toHaveCount(1)
  await expectNoHorizontalScroll(page)
})

test("when the payment takes too long, a warning says what to do and can check again", async ({
  page,
  request,
}) => {
  await page.clock.install()
  const id = await placeOrder(request)
  // The watcher sets its 2-minute deadline after hydration, but the notice is server HTML seen
  // before it; its first re-read (router.refresh, not a prefetch) proves the deadline is set.
  const polled = page.waitForRequest(
    (r) =>
      new URL(r.url()).pathname === `/orders/${id}` &&
      r.headers()["rsc"] === "1" &&
      !r.headers()["next-router-prefetch"]
  )
  await page.goto(`/orders/${id}`)
  await expect(notice(page, "info", "Waiting for payment.")).toBeVisible()
  await polled

  await page.clock.fastForward("02:01")
  const warning = notice(page, "warning", "Still waiting for the payment")
  await expect(warning).toBeVisible()
  await expect(warning).toContainText(
    "The provider has not confirmed payment yet. Check again in a minute or contact us before trying another payment."
  )
  await expectNoHorizontalScroll(page)

  await warning.getByRole("button", { name: "Check again" }).click()
  await expect(warning).toHaveCount(0)
  await expect(notice(page, "info", "Waiting for payment.")).toBeVisible()
})

test("an expired order is a warning with a way back to the shop", async ({
  page,
  request,
}) => {
  await page.goto(`/orders/${await placeOrder(request, EXPIRED_EMAIL)}`)
  const warning = notice(
    page,
    "warning",
    "This order expired before it was paid"
  )
  await expect(warning).toContainText("The items went back on the shelf.")
  await expect(page.getByRole("button", { name: /^Pay \$\d/ })).toHaveCount(0)
  await warning.getByRole("link", { name: "Shop again" }).click()
  await expect(page).toHaveURL("/shop")
})

test("a paid order the shop refunded and cancelled says so, not that it expired", async ({
  page,
  request,
}) => {
  await page.goto(`/orders/${await placeOrder(request, REFUNDED_EMAIL)}`)
  const notice = page
    .getByRole("status")
    .locator('[data-variant="info"]')
    .filter({ hasText: "This order was cancelled" })
  await expect(notice).toContainText("Your payment has been refunded")
  await expect(page.getByText(/expired before it was paid/)).toHaveCount(0)
  await expect(page.getByText("Cancelled", { exact: true })).toBeVisible()
  await expect(page.getByRole("button", { name: /^Pay \$\d/ })).toHaveCount(0)
  await expectNoHorizontalScroll(page)
  await notice.getByRole("link", { name: "Shop again" }).click()
  await expect(page).toHaveURL("/shop")
})

test("an uncertain payment start waits for review without offering another payment", async ({
  page,
  request,
}) => {
  await page.goto(`/orders/${await placeOrder(request, PROVIDER_DOWN_EMAIL)}`)
  await page.getByRole("button", { name: /^Pay \$\d/ }).click()
  const warning = notice(page, "warning", "Payment pending review")
  await expect(warning).toContainText(
    "Please wait or contact us before trying another payment"
  )
  await expect(page.getByText("Nothing was charged.")).toHaveCount(0)
  await expect(
    page.getByRole("button", { name: /^Pay \$|Try again/ })
  ).toHaveCount(0)
  await expectNoHorizontalScroll(page)
})

test.describe("order that can't be loaded", () => {
  test.use({
    allow: {
      responses: [{ url: `/orders/${FAILING_ORDER}`, status: 500 }],
      console: [
        "Error: Minified React error #441; visit https://react.dev/errors/441 for the full message or use the non-minified dev environment for full errors and additional helpful warnings.",
      ],
    },
  })

  test("is an error with a retry, and says the order is safe", async ({
    page,
  }) => {
    const response = await page.goto(`/orders/${FAILING_ORDER}`)
    expect(response?.status()).toBe(500)
    const error = page
      .getByRole("alert")
      .filter({ hasText: "We couldn't load your order" })
    await expect(error).toContainText("Your order is safe.")
    await expect(error.getByRole("button", { name: "Try again" })).toBeVisible()
  })
})
