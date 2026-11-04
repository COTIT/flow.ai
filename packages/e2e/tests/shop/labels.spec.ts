import { expect, expectNoRawValues, test } from "../../src/test"
import { placeOrder } from "./orders"

// The order page says every status in words, never the API's code (`partially_returned`).
// Legit codes here: the order number.
for (const status of [
  "draft",
  "open",
  "completed",
  "partially_returned",
  "returned",
  "refunded",
])
  test(`an order that is ${status} shows its status in words`, async ({
    page,
    request,
  }) => {
    await page.goto(`/orders/${await placeOrder(request, `status-${status}@e2e.test`)}`)
    // A paid order (open, completed) shows its paid badge; the rest a status line.
    await expect(page.getByText(/^Status: |^Paid$/).first()).toBeVisible()
    await expectNoRawValues(page)
  })
