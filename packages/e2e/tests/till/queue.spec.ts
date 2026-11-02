import type { Page } from "@playwright/test"

import { expectNoHorizontalScroll } from "../../src/test"
import { expect, honey, seedTill, test, type FakeApi, WS } from "./fake-till"

const REF = "00000000-0000-4000-8000-000000000001"
const salesUrl = `/v1/workspace/${WS}/till/sales`
const refUrl = `${salesUrl}/${REF}`

const sale = {
  order_id: "o-1",
  number: "1042",
  client_ref: REF,
  register_id: "reg-1",
  till_user_id: "tu-1",
  method: "cash",
  total: 2502,
  server_total: 2502,
  price_mismatch: 0,
  mismatched_lines: 0,
  due: 2500,
  rounding: -2,
  tendered: 3000,
  change: 500,
  card_reference: null,
  sold_at: "2026-11-01T09:12:00Z",
}

/** Every sale this page rings up gets REF as its client_ref. */
async function fixedRef(page: Page) {
  await page.addInitScript((ref) => {
    crypto.randomUUID = () =>
      ref as `${string}-${string}-${string}-${string}-${string}`
  }, REF)
}

/** A server that has no sale under REF until it is posted. */
function serverApi(api: FakeApi) {
  let booked = false
  api
    .on("GET", "/workspace/:ws/till/sales/:ref", () =>
      booked ? { data: sale } : { status: 404, body: { message: "Not found." } }
    )
    .on("POST", "/workspace/:ws/till/sales", () => {
      booked = true
      return { status: 201, body: { data: sale } }
    })
}

async function ringUpCash(page: Page) {
  await page.getByRole("button", { name: /Ilundo honey 500 g/ }).click()
  await page
    .getByRole("button", { name: "One more Ilundo honey 500 g" })
    .click()
}

/** Opens the payment sheet and types €30.00 received. */
async function pay30(page: Page) {
  await page.getByRole("button", { name: /^Charge/ }).click()
  for (const d of "3000")
    await page.getByRole("button", { name: d, exact: true }).click()
}

test.describe("offline", () => {
  test.use({ allow: { responses: [{ url: refUrl, status: 404 }] } })

  test("a sale rung up offline is kept and sent once when the till is back online", async ({
    page,
    context,
    api,
  }) => {
    serverApi(api)
    await fixedRef(page)
    await seedTill(page)
    await page.goto("/sale")
    await ringUpCash(page)

    await context.setOffline(true)
    // Offline the basket can't ask for a quote: it shows the labelled estimate (catalog gross × pieces).
    const basket = page.getByRole("complementary", { name: "Basket" })
    await expect(basket.getByText("Estimate (offline)")).toBeVisible()
    await expect(basket.getByRole("definition")).toHaveText("≈ €25.00")
    await pay30(page)
    // No quote offline: no change shown, and Exact has no due to match.
    await expect(page.getByLabel("Change")).toHaveText("—")
    await expect(page.getByRole("button", { name: "Exact" })).toBeDisabled()
    await page.getByRole("button", { name: /^Book sale/ }).click()
    const saved = page.getByRole("dialog", { name: "Saved offline" })
    await expect(saved.getByText("≈ €25.00")).toBeVisible()
    await expect(page.getByText("Offline · 1 to sync")).toBeVisible()
    await expectNoHorizontalScroll(page)
    await saved.getByRole("button", { name: "New sale" }).click()
    expect(api.callsTo("POST", `/workspace/${WS}/till/sales`)).toHaveLength(0)

    await context.setOffline(false)
    await expect(page.getByText("Offline sales synced")).toBeVisible()
    const posts = api.callsTo("POST", `/workspace/${WS}/till/sales`)
    expect(posts).toHaveLength(1)
    const body = posts[0]!.body as Record<string, unknown>
    expect(body.client_ref).toBe(REF)
    expect(body.sold_at).toEqual(expect.any(String))
    expect(body.payment).toEqual({ method: "cash", tendered: 3000 })
    // Reconcile first: the replay asked whether the server already had the sale.
    expect(
      api.callsTo("GET", `/workspace/${WS}/till/sales/${REF}`)
    ).toHaveLength(1)

    await page.getByText("Offline sales synced").click()
    await expect(page).toHaveURL("/queue")
    await expect(page.getByText("Sale 1042: €25.00")).toBeVisible()
    await expect(page.getByText("Booked", { exact: true })).toBeVisible()
    await expect(
      page
        .getByRole("status")
        .filter({ hasText: "All offline sales are booked." })
    ).toBeVisible()
  })
})

test.describe("lost answer", () => {
  test.use({ allow: { failures: [salesUrl] } })

  test("a sale whose reply got lost is found on the server, not posted again", async ({
    page,
    api,
  }) => {
    serverApi(api)
    // The server books the sale, but the reply never reaches the till.
    let dropped = false
    await page.route(`**${salesUrl}`, async (route) => {
      if (dropped) return route.fallback()
      dropped = true
      api.on("GET", "/workspace/:ws/till/sales/:ref", () => ({ data: sale }))
      await route.abort("connectionreset")
    })
    await fixedRef(page)
    await seedTill(page)
    await page.goto("/sale")
    await ringUpCash(page)
    await pay30(page)
    await page.getByRole("button", { name: /^Book sale/ }).click()

    await expect(
      page.getByRole("dialog", { name: "Saved offline" })
    ).toBeVisible()
    await expect(page.getByText("Offline sales synced")).toBeVisible()
    expect(
      api.callsTo("GET", `/workspace/${WS}/till/sales/${REF}`)
    ).toHaveLength(1)
    expect(api.callsTo("POST", `/workspace/${WS}/till/sales`)).toHaveLength(0)
  })
})

/** A queued sale already on the device, from an earlier offline spell. */
async function seedQueue(page: Page) {
  const item = {
    workspace: WS,
    body: {
      client_ref: REF,
      sold_at: "2026-11-01T09:00:00Z",
      lines: [
        { product_id: honey.id, quantity: "1", unit_price: honey.unit_price },
      ],
      payment: { method: "card", reference: null },
    },
    lines: [
      {
        productId: honey.id,
        variantId: null,
        name: honey.name,
        unitNet: honey.unit_price,
        unitGross: honey.unit_price_gross,
        quantity: 1,
      },
    ],
    status: "pending",
  }
  await page.addInitScript(
    (v) => localStorage.setItem("genesy.till.queue", v),
    JSON.stringify({ state: { items: [item] }, version: 0 })
  )
}

test.describe("refused replay", () => {
  test.use({
    allow: {
      responses: [
        { url: refUrl, status: 404 },
        { url: salesUrl, status: 409 },
      ],
    },
  })

  test("a client_ref the server used for another sale is marked as a conflict", async ({
    page,
    api,
  }) => {
    serverApi(api)
    api.on("POST", "/workspace/:ws/till/sales", () => ({
      status: 409,
      body: {
        message: "This client_ref was used for another sale on this register.",
      },
    }))
    await seedQueue(page)
    await seedTill(page)
    await page.goto("/queue")
    await expect(page.getByText("Conflict", { exact: true })).toBeVisible()
    await expect(
      page
        .getByRole("alert")
        .filter({ hasText: "Show it to the office before dismissing it." })
    ).toBeVisible()
    await expect(
      page.getByText(
        "This client_ref was used for another sale on this register."
      )
    ).toBeVisible()
    await page.getByRole("button", { name: "Dismiss" }).click()
    await expect(page.getByText("No offline sales.")).toBeVisible()
  })
})

test.describe("server down", () => {
  test.use({ allow: { responses: [{ url: refUrl, status: 503 }] } })

  test("closing the shift waits until queued sales are synced", async ({
    page,
    api,
  }) => {
    api
      .on("GET", "/workspace/:ws/till/sales/:ref", () => ({
        status: 503,
        body: { message: "The record is busy, please try again." },
      }))
      .on("GET", "/workspace/:ws/till/shift", () => ({
        data: {
          id: "sh-1",
          register_id: "reg-1",
          opened_by: "tu-1",
          opened_at: "2026-11-01T08:00:00Z",
          opening: 4515,
          expected: 4515,
          closed_by: null,
          closed_at: null,
          counted: null,
          difference: null,
        },
      }))
    await seedQueue(page)
    await seedTill(page)
    await page.goto("/drawer")
    await expect(
      page.getByRole("status").filter({
        hasText: "1 offline sale still to sync. Sync before closing the shift.",
      })
    ).toBeVisible()
    await expect(
      page.getByRole("button", { name: "Close shift" })
    ).toBeDisabled()
    await expect(page.getByText("1 sale to sync")).toBeVisible()
  })
})

test("an empty queue says what it is for", async ({ page }) => {
  await seedTill(page)
  await page.goto("/queue")
  await expect(page.getByText("No offline sales.")).toBeVisible()
  await expect(
    page.getByText("Sales rung up without a connection wait here")
  ).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test("a replayed sale booked at other prices is flagged", async ({
  page,
  api,
}) => {
  api.on("GET", "/workspace/:ws/till/sales/:ref", () => ({
    data: { ...sale, price_mismatch: -100, mismatched_lines: 1 },
  }))
  await seedQueue(page)
  await seedTill(page)
  await page.goto("/queue")
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "All offline sales are booked." })
  ).toBeVisible()
  await expect(
    page.getByRole("status").filter({
      hasText: "Charged at prices that differ from the current catalog.",
    })
  ).toBeVisible()
})
