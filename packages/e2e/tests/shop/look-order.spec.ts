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

  // The API's lines in the left tile: neutral 44 px tile, title, "qty × unit", line total.
  const lines = card.getByRole("list", { name: "Items" })
  const line = lines.getByRole("listitem")
  await expect(line).toHaveCount(1)
  await expect(line).toContainText("Ilundo Wild Forest Honey · 500 g")
  await expect(line).toContainText("1 × $24.99")
  await expect(line.getByText("$24.99", { exact: true })).toBeVisible()
  const thumb = line.locator("[aria-hidden]").first()
  expect(await css(thumb, "width")).toBe("44px")
  expect(await css(thumb, "height")).toBe("44px")
  // Side by side on a desktop (grid-2), stacked on a phone.
  const linesBox = (await lines.boundingBox())!
  const totalsBox = (await totals.boundingBox())!
  if (isMobile)
    expect(totalsBox.y).toBeGreaterThan(linesBox.y + linesBox.height)
  else {
    expect(totalsBox.x).toBeGreaterThan(linesBox.x + linesBox.width)
    expect(Math.round(totalsBox.y)).toBe(Math.round(linesBox.y))
  }

  await expectNoHorizontalScroll(page)
  if (isMobile) await expectTapTargets(page)
  // On a phone the hold time shifts the page (see the test below).
  if (!isMobile) await expectNoLayoutShift(page)
})

test("awaiting payment: nothing moves once the hold time appears", async ({
  page,
  request,
}) => {
  await page.goto(`/orders/${await placeOrder(request)}`)
  await expect(page.getByText(/Your items are reserved until \d/)).toBeVisible()
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
