import {
  EXPIRED_EMAIL,
  FAILING_ORDER,
  PROVIDER_DOWN_EMAIL,
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
  await page.goto(`/orders/${await placeOrder(request)}`)
  await expect(notice(page, "info", "Waiting for payment.")).toBeVisible()

  await page.clock.fastForward("02:01")
  const warning = notice(page, "warning", "Still waiting for the payment")
  await expect(warning).toBeVisible()
  await expect(warning).toContainText("If you paid, check again in a minute")
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
  await expect(page.getByRole("button", { name: "Pay now" })).toHaveCount(0)
  await warning.getByRole("link", { name: "Shop again" }).click()
  await expect(page).toHaveURL("/")
})

test("a failed payment start is an error, and the button offers to try again", async ({
  page,
  request,
}) => {
  await page.goto(`/orders/${await placeOrder(request, PROVIDER_DOWN_EMAIL)}`)
  await page.getByRole("button", { name: "Pay now" }).click()
  const error = page
    .getByRole("alert")
    .filter({ hasText: "The payment service isn't answering" })
  await expect(error).toContainText("Nothing was charged.")
  await expect(error).toHaveAttribute("data-variant", "error")
  await expect(page.getByRole("button", { name: "Try again" })).toBeEnabled()
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
