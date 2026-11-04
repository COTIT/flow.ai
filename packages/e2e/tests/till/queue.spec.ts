import type { Page } from "@playwright/test"

import { expectNoRawValues } from "../../src/test"
import { expectTillLook } from "./look"
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
  lines: [
    {
      title: honey.name,
      product_id: honey.id,
      product_variant_id: null,
      quantity: "2.00",
      unit_price: 1147,
      line_excl: 2294,
      line_vat: 208,
      line_total: 2502,
    },
  ],
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
    await expectTillLook(page, { shift: false })
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
    await expect(page.getByText("Booked as sale 1042 · €25.00")).toBeVisible()
    await expect(page.getByText("Booked", { exact: true })).toBeVisible()
    await expect(
      page
        .getByRole("status")
        .filter({ hasText: "All offline sales are booked." })
    ).toBeVisible()

    // Sync now, a reload and the 30 s timer replay only what still waits: nothing here.
    await page.getByRole("button", { name: "Sync now" }).click()
    await page.reload()
    await expect(page.getByText("Booked as sale 1042 · €25.00")).toBeVisible()
    expect(api.callsTo("POST", `/workspace/${WS}/till/sales`)).toHaveLength(1)
    expect(
      api.callsTo("GET", `/workspace/${WS}/till/sales/${REF}`)
    ).toHaveLength(1)
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
    // The API's sentence names an internal field; the cashier reads it in their own words.
    await expect(
      page.getByText(
        "This sale's number was already used for another sale. Show it to the office."
      )
    ).toBeVisible()
    await expectNoRawValues(page)
    await expect(page.getByText("Needs the office")).toBeVisible()
    // The replay's answer turned the card into a conflict after load: shift checked elsewhere.
    await expectTillLook(page, { shift: false })
    await page.getByRole("button", { name: "Dismiss" }).click()
    await expect(page.getByText("All offline sales are booked")).toBeVisible()
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
  await expect(page.getByText("All offline sales are booked")).toBeVisible()
  await expect(
    page.getByText("Sales rung up without a connection wait here")
  ).toBeVisible()
  await expectTillLook(page)
})

test.describe("server busy, then offline", () => {
  test.use({ allow: { responses: [{ url: refUrl, status: 503 }] } })

  test("the queue shows its waiting sales and Sync now in the warning", async ({
    page,
    context,
    api,
  }) => {
    api.on("GET", "/workspace/:ws/till/sales/:ref", () => ({
      status: 503,
      body: { message: "Busy." },
    }))
    await seedQueue(page)
    await seedTill(page)
    await page.goto("/queue")
    const card = page.getByRole("listitem").filter({ hasText: "1 item" })
    // Go offline only once the page is up: the offline event must reach a mounted listener.
    await expect(card).toBeVisible()
    await context.setOffline(true)
    const warning = page
      .getByRole("status")
      .filter({ hasText: "Offline — sales are kept on this device" })
    await expect(warning).toHaveCount(1)
    await expect(
      warning.getByRole("button", { name: "Sync now" })
    ).toBeVisible()
    await expect(card.getByText("Waiting", { exact: true })).toBeVisible()
    await expect(card.getByText("Card · ≈ €12.50 estimate")).toBeVisible()
    await expect(page.getByText("1 waiting")).toBeVisible()
    await expect(page.getByText("Offline · 1 to sync")).toBeVisible()
    await expectTillLook(page, { shift: false })
  })
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

test.describe("replay refused as invalid", () => {
  test.use({
    allow: {
      responses: [
        { url: refUrl, status: 404 },
        { url: salesUrl, status: 422 },
      ],
    },
  })

  test("a 422 marks the sale for review with the API's reason, and it is not retried", async ({
    page,
    api,
  }) => {
    serverApi(api)
    // Rung up after the product went off sale: the API refuses the replay too.
    api.on("POST", "/workspace/:ws/till/sales", () => ({
      status: 422,
      body: {
        message: "This product is not for sale.",
        errors: { "lines.0.product_id": ["This product is not for sale."] },
      },
    }))
    await seedQueue(page)
    await seedTill(page)
    await page.goto("/queue")
    const card = page.getByRole("listitem").filter({ hasText: "1 item" })
    await expect(card.getByText("Needs review", { exact: true })).toBeVisible()
    await expect(
      card
        .getByRole("alert")
        .filter({ hasText: "This product is not for sale." })
    ).toBeVisible()
    await expect(page.getByText("1 needs the office")).toBeVisible()
    await expectTillLook(page, { shift: false })

    await page.getByRole("button", { name: "Sync now" }).click()
    await expect(card.getByText("Needs review", { exact: true })).toBeVisible()
    expect(api.callsTo("POST", `/workspace/${WS}/till/sales`)).toHaveLength(1)
  })
})

test.describe("replay of a product since taken off sale", () => {
  test.use({ allow: { responses: [{ url: refUrl, status: 404 }] } })

  test("rung up before the change, it is booked and the till flags it for the office", async ({
    page,
    api,
  }) => {
    serverApi(api)
    api.on("POST", "/workspace/:ws/till/sales", () => ({
      status: 201,
      body: { data: { ...sale, unsellable_lines: 1 } },
    }))
    await seedQueue(page)
    await seedTill(page)
    await page.goto("/queue")
    const card = page.getByRole("listitem").filter({ hasText: "1 item" })
    await expect(card.getByText("Booked", { exact: true })).toBeVisible()
    // A warning on the card itself, like the price-mismatch one.
    await expect(
      card.getByRole("status").filter({
        hasText: "A product in this sale is no longer for sale.",
      })
    ).toBeVisible()
  })
})
