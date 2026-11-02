import type { Locator, Page } from "@playwright/test"

import { EXPIRED_EMAIL } from "../../mock/fixtures.mjs"
import {
  expect,
  expectNoHorizontalScroll,
  expectNoLayoutShift,
  expectTapTargets,
  test,
} from "../../src/test"
import { placeOrder } from "./orders"

// Mock `ShopOrder` (status awaiting / paid / cancelled) in docs/design/web-redesign/canvas.
const css = (l: Locator, prop: string) =>
  l.evaluate((el, p) => getComputedStyle(el).getPropertyValue(p), prop)

/** The one order card, centred at 920 px. */
const orderCard = (page: Page) =>
  page
    .locator("main section")
    .filter({ has: page.getByRole("heading", { level: 1 }) })

test("awaiting payment: gold badge, info notice with the pay button inside, totals", async ({
  page,
  request,
  isMobile,
}) => {
  await page.goto(`/orders/${await placeOrder(request)}`)
  const card = orderCard(page)
  if (!isMobile) expect(await css(card, "max-width")).toBe("920px")

  const h1 = page.getByRole("heading", { level: 1, name: /^Order WEB-\d+$/ })
  await expect(h1).toBeVisible()
  await expect(
    card.getByText("Awaiting payment", { exact: true })
  ).toBeVisible()

  const info = card.locator('[data-variant="info"]')
  await expect(info).toContainText("Waiting for payment.")
  const pay = info.getByRole("button", { name: /^Pay \$\d+\.\d\d$/ })
  expect(await css(pay, "height")).toBe("52px")

  const totals = card.locator("dl")
  await expect(totals).toContainText("Subtotal")
  await expect(totals).toContainText("VAT")
  await expect(totals).toContainText("Total")

  await expectNoHorizontalScroll(page)
  if (isMobile) await expectTapTargets(page)
  await expectNoLayoutShift(page)
})

test("paid: teal badge and the thank-you notice", async ({ page, request }) => {
  const id = await placeOrder(request)
  await page.goto(`/orders/${id}`)
  const back = page.waitForResponse(
    (r) =>
      r.request().isNavigationRequest() && r.url().endsWith(`/orders/${id}`)
  )
  await page.getByRole("button", { name: /^Pay \$/ }).click()
  await back
  const card = orderCard(page)
  await expect(card.getByText("Paid", { exact: true })).toBeVisible()
  await expect(card.locator('[data-variant="success"]')).toContainText(
    "Thank you! Your payment is in"
  )
  await expect(page.getByRole("button", { name: /^Pay \$/ })).toHaveCount(0)
  await expectNoHorizontalScroll(page)
})

test("cancelled: red badge, warning with a small Shop again", async ({
  page,
  request,
}) => {
  await page.goto(`/orders/${await placeOrder(request, EXPIRED_EMAIL)}`)
  const card = orderCard(page)
  await expect(card.getByText("Cancelled", { exact: true })).toBeVisible()
  const warning = card.locator('[data-variant="warning"]')
  await expect(warning.getByRole("link", { name: "Shop again" })).toBeVisible()
  await expectNoHorizontalScroll(page)
})
