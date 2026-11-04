import type { Page } from "@playwright/test"

import { mockApi } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, role, villages } from "./api"
import {
  batchInvalid,
  batchMade,
  batchOptions,
  batches,
  crops,
  currencies,
  dispatched,
  dispatchOptions,
  granted,
  honeyJar,
  links,
  lotStock,
  notLive,
  officeOnly,
  nothingToBuy,
  ownVerification,
  rawCoffee,
  rawProductMade,
  transfers,
  unavailable,
} from "./supply.api"

const KEY = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/

type Reply = { status: number; body: unknown }

/**
 * Signed in as ops. The n-th transfer is answered with the n-th reply (the last one repeats).
 * Returns how many transfers reached the API and the Idempotency-Key each carried.
 */
async function open(page: Page, path: string, dispatchReplies: Reply[] = [{ status: 201, body: dispatched }], locale: "en" | "sw" = "en") {
  await seedSession(page)
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["ops:u1"])))
  await mockApi(page, {
    "GET /farm/me": me({ name: "Juma Ops", locale, memberships: [role("ops", null)] }),
    "GET /workspace/w1/farm/villages": villages,
    "GET /workspace/w1/supply-links": links,
    "POST /workspace/w1/supply-links": granted,
    "GET /workspace/w1/supply-transfers": transfers,
    "GET /workspace/w1/farm/supply-batches": batches,
    "GET /workspace/w1/farm/dispatch/options": dispatchOptions,
    "GET /workspace/w1/warehouses": officeOnly,
    "GET /workspace/w1/lots/lot1/stock": lotStock,
    "GET /workspace/w1/farm/crops": crops,
    "GET /workspace/w1/currencies": currencies,
  })
  // Registered after mockApi, so it answers first: counts sends, and answers slowly so a second tap lands in flight.
  const sends = { count: 0, keys: [] as (string | undefined)[] }
  await page.route("**/v1/workspace/w1/lots/lot1/transfers", async (route) => {
    sends.count++
    sends.keys.push(await route.request().headerValue("idempotency-key") ?? undefined)
    const dispatchReply = dispatchReplies[Math.min(sends.count, dispatchReplies.length) - 1]!
    await new Promise((resolve) => setTimeout(resolve, 400))
    await route.fulfill({
      status: dispatchReply.status,
      contentType: "application/json",
      body: JSON.stringify(dispatchReply.body),
    })
  })
  await page.goto(path)
  return sends
}

async function fill(page: Page) {
  await page.getByTestId("dispatch-link").selectOption({ label: "AFM shop" })
  await page.getByTestId("dispatch-lot").selectOption({ label: "ILU-H-2026-01" })
  await page.getByTestId("dispatch-warehouse").selectOption({ label: "Iringa store" })
  await page.getByTestId("dispatch-quantity").fill("12.50")
}

test("ops see the links they granted and grant one to the shop", async ({ page }) => {
  await open(page, "/ops/supply-links")

  const rows = page.getByTestId("supply-link-row")
  await expect(rows).toHaveCount(2)
  await expect(rows.first()).toContainText("AFM shop")
  await expect(rows.first()).toContainText("Live")
  await expect(rows.first()).toContainText("Village, Number of farmers")
  await expect(rows.nth(1)).toContainText("Revoked")
  await expect(rows.nth(1).getByTestId("supply-link-revoke")).toHaveCount(0)

  await page.getByTestId("supply-grant-to").fill("w-shop")
  await page.getByTestId("supply-grant-field-village").check()
  const sent = page.waitForRequest(
    (r) => r.method() === "POST" && r.url().endsWith("/v1/workspace/w1/supply-links")
  )
  await page.getByTestId("supply-grant-submit").click()
  const request = await sent
  expect(request.postDataJSON()).toEqual({ to_workspace_id: "w-shop", fields: ["village"] })
  // Makes a retry after a lost reply safe: the API replays the first answer.
  expect(await request.headerValue("idempotency-key")).toMatch(KEY)
  await expect(page.getByTestId("supply-grant-to")).toHaveValue("")
  await expectNoHorizontalScroll(page)
})

test("a dispatch goes through the dialog: cancel sends nothing, confirm sends one", async ({ page }) => {
  const sends = await open(page, "/ops/dispatch")

  const row = page.getByTestId("supply-transfer-row")
  await expect(row).toContainText("ILU-H-2026-01 → AFM shop")
  await expect(row).toContainText("Received 39.50")

  await fill(page)
  // The picked lot's stock, for reading: the API still refuses an over-send.
  await expect(page.getByTestId("dispatch-stock-row")).toHaveText("Iringa store: 35.00 kg available · 40.00 kg on hand, 5.00 kg held for orders")
  await page.getByTestId("dispatch-send").click()
  await expect(page.getByTestId("confirm-dialog")).toContainText("12.50 of lot ILU-H-2026-01 to AFM shop")
  await page.getByTestId("confirm-dialog-cancel").click()
  await expect(page.getByTestId("confirm-dialog")).toHaveCount(0)
  expect(sends.count).toBe(0)

  await page.getByTestId("dispatch-send").click()
  const sent = page.waitForRequest("**/v1/workspace/w1/lots/lot1/transfers")
  await page.getByTestId("confirm-dialog-confirm").dblclick()
  expect((await sent).postDataJSON()).toEqual({ link_id: "l1", warehouse_id: "wh1", quantity: "12.50" })
  await expect(page.getByTestId("dispatch-sent")).toBeVisible()
  expect(sends.count).toBe(1)
  expect(sends.keys[0]).toMatch(KEY)
  await expectNoHorizontalScroll(page)
})

test.describe("a dispatch whose reply was lost", () => {
  test.use({ allow: { responses: [{ url: "/v1/workspace/w1/lots/lot1/transfers", status: 503 }] } })

  test("is resent under the same Idempotency-Key, so the API replays one transfer", async ({ page }) => {
    const sends = await open(page, "/ops/dispatch", [
      { status: 503, body: unavailable },
      { status: 201, body: dispatched },
    ])
    await fill(page)
    await page.getByTestId("dispatch-send").click()
    await page.getByTestId("confirm-dialog-confirm").click()
    await expect(page.getByTestId("dispatch-error")).toBeVisible()

    await page.getByTestId("dispatch-send").click()
    await page.getByTestId("confirm-dialog-confirm").click()
    await expect(page.getByTestId("dispatch-sent")).toBeVisible()
    expect(sends.count).toBe(2)
    expect(sends.keys[0]).toMatch(KEY)
    expect(sends.keys[1]).toBe(sends.keys[0])
    await expectNoHorizontalScroll(page)
  })
})

test.describe("a refused dispatch", () => {
  test.use({ allow: { responses: [{ url: "/v1/workspace/w1/lots/lot1/transfers", status: 403 }] } })

  test("shows the API's message as written", async ({ page }) => {
    const sends = await open(page, "/ops/dispatch", [{ status: 403, body: notLive }])
    await fill(page)
    await page.getByTestId("dispatch-send").click()
    await page.getByTestId("confirm-dialog-confirm").click()

    await expect(page.getByTestId("dispatch-error")).toContainText("The supply link is not live.")
    await expect(page.getByTestId("confirm-dialog")).toHaveCount(0)
    expect(sends.count).toBe(1)
    await expectNoHorizontalScroll(page)
  })
})

test.describe("buying harvest when the crop has no raw product yet", () => {
  test("ops create one in the dialog, it is selected, and the batch is bought", async ({ page }) => {
    await open(page, "/ops/dispatch")
    // The options read answers without a coffee product until one is made.
    let made = false
    await page.route("**/v1/workspace/w1/farm/supply-batches/options**", (route) =>
      route.fulfill({
        contentType: "application/json",
        body: JSON.stringify(batchOptions({ products: made ? [rawCoffee, honeyJar] : [honeyJar], canCreate: true }).body),
      }),
    )
    await page.route("**/v1/workspace/w1/farm/supply-batches/raw-product", async (route) => {
      made = true
      await route.fulfill({ status: 201, contentType: "application/json", body: JSON.stringify(rawProductMade.body) })
    })
    await page.route("**/v1/workspace/w1/farm/supply-batches", (route) =>
      route.request().method() === "POST"
        ? route.fulfill({ status: 201, contentType: "application/json", body: JSON.stringify(batchMade.body) })
        : route.fallback(),
    )

    await page.getByTestId("batch-village").selectOption({ label: "Ilundo" })
    await page.getByTestId("batch-crop").selectOption({ label: "Coffee" })
    await expect(page.getByTestId("batch-no-raw-product")).toContainText("No raw product for Coffee yet")
    await expectNoHorizontalScroll(page)

    await page.getByTestId("batch-create-raw").click()
    await expect(page.getByTestId("raw-product-name")).toHaveValue("Raw Coffee")
    const rawSent = page.waitForRequest((r) => r.method() === "POST" && r.url().endsWith("/farm/supply-batches/raw-product"))
    await page.getByTestId("confirm-dialog-confirm").click()
    expect((await rawSent).postDataJSON()).toEqual({ name: "Raw Coffee" })
    await expect(page.getByTestId("confirm-dialog")).toHaveCount(0)
    await expect(page.getByTestId("batch-product")).toHaveValue("p9")
    await expect(page.getByTestId("batch-no-raw-product")).toHaveCount(0)

    await expect(page.getByTestId("batch-kg-h1")).toHaveValue("200.00")
    await page.getByTestId("batch-code").fill("ILU-C-01")
    await page.getByTestId("batch-price").fill("8000")
    await page.getByTestId("batch-submit").click()
    const batchSent = page.waitForRequest((r) => r.method() === "POST" && r.url().endsWith("/farm/supply-batches"))
    await page.getByTestId("confirm-dialog-confirm").click()
    const body = (await batchSent).postDataJSON()
    expect(body).toMatchObject({
      village_id: "v1",
      crop_id: "crop1",
      code: "ILU-C-01",
      raw_product_id: "p9",
      price_per_kg: 800000,
      currency_id: "tzs",
      lines: [{ harvest_report_id: "h1", kg: "200.00" }],
    })
    expect(body.client_ref).toMatch(KEY)
    await expect(page.getByTestId("batch-made")).toContainText("ILU-C-01")
    await expectNoHorizontalScroll(page)
  })

  test("without the right, the notice says who can add one and offers no button", async ({ page }) => {
    await open(page, "/ops/dispatch")
    await page.route("**/v1/workspace/w1/farm/supply-batches/options**", (route) =>
      route.fulfill({ contentType: "application/json", body: JSON.stringify(batchOptions({ products: [], canCreate: false }).body) }),
    )
    await page.getByTestId("batch-village").selectOption({ label: "Ilundo" })
    await page.getByTestId("batch-crop").selectOption({ label: "Coffee" })
    await expect(page.getByTestId("batch-no-raw-product")).toContainText("Office → Products")
    await expect(page.getByTestId("batch-create-raw")).toHaveCount(0)
    await expectNoHorizontalScroll(page)
  })
})

/** Ops on /ops/dispatch with one harvest to buy (or `options`); the n-th batch POST gets the n-th answer. Returns the bodies sent. */
async function buying(
  page: Page,
  answers: Reply[],
  options = batchOptions({ products: [honeyJar, rawCoffee], canCreate: true }),
  locale: "en" | "sw" = "en",
) {
  await open(page, "/ops/dispatch", undefined, locale)
  await page.route("**/v1/workspace/w1/farm/supply-batches/options**", (route) =>
    route.fulfill({ status: options.status ?? 200, contentType: "application/json", body: JSON.stringify(options.body) }),
  )
  const sent: Record<string, unknown>[] = []
  await page.route("**/v1/workspace/w1/farm/supply-batches", async (route) => {
    if (route.request().method() !== "POST") return route.fallback()
    sent.push(route.request().postDataJSON())
    const answer = answers[Math.min(sent.length, answers.length) - 1]!
    await route.fulfill({ status: answer.status, contentType: "application/json", body: JSON.stringify(answer.body) })
  })
  await page.getByTestId("batch-village").selectOption({ label: "Ilundo" })
  await page.getByTestId("batch-crop").selectOption({ label: locale === "sw" ? "Kahawa" : "Coffee" })
  return sent
}

async function buy(page: Page) {
  await page.getByTestId("batch-product").selectOption({ label: "Raw Coffee (kg)" })
  await page.getByTestId("batch-code").fill("ILU-C-01")
  await page.getByTestId("batch-price").fill("8000")
  await page.getByTestId("batch-submit").click()
  await page.getByTestId("confirm-dialog-confirm").click()
}

test("ops buy verified harvest with an existing raw product: the farmer's kg left, partly bought", async ({ page }) => {
  const sent = await buying(page, [batchMade as Reply])
  await expect(page.getByTestId("batch-no-raw-product")).toHaveCount(0)
  await expect(page.getByTestId("batch-line-h1")).toContainText("Neema Mwakalinga · Shamba A · 200.00 kg left")
  await page.getByTestId("batch-kg-h1").fill("150.50")
  await buy(page)
  await expect(page.getByTestId("batch-made")).toContainText("Bought into lot ILU-C-01.")
  expect(sent).toHaveLength(1)
  expect(sent[0]).toMatchObject({ raw_product_id: "p9", price_per_kg: 800000, lines: [{ harvest_report_id: "h1", kg: "150.50" }] })
  await expectNoHorizontalScroll(page)
})

test("with nothing left to buy, the card says why and cannot be sent", async ({ page }) => {
  const sent = await buying(page, [batchMade as Reply], nothingToBuy)
  await expect(page.getByTestId("batch-no-harvests")).toContainText("No harvest to buy")
  await page.getByTestId("batch-product").selectOption({ label: "Raw Coffee (kg)" })
  await page.getByTestId("batch-code").fill("ILU-C-01")
  await page.getByTestId("batch-price").fill("8000")
  await expect(page.getByTestId("batch-submit")).toBeDisabled()
  expect(sent).toHaveLength(0)
  await expectNoHorizontalScroll(page)
})

test("a buyer reads the card in Swahili", async ({ page }) => {
  await buying(page, [batchMade as Reply], nothingToBuy, "sw")
  await expect(page.getByRole("heading", { name: "Nunua mavuno" })).toBeVisible()
  await expect(page.getByTestId("batch-no-harvests")).toContainText("Hakuna mavuno ya kununua")
  await expectNoHorizontalScroll(page)
})

test.describe("a refused batch", () => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/farm/supply-batches", status: 422 },
        { url: "/v1/workspace/w1/farm/supply-batches", status: 403 },
        { url: "/v1/workspace/w1/farm/supply-batches", status: 503 },
      ],
    },
  })

  test("422: each message sits at its own field, the harvest's at its kg", async ({ page }) => {
    await buying(page, [batchInvalid as Reply])
    await buy(page)
    await expect(page.getByTestId("batch-code-error")).toContainText("The code has already been taken.")
    await expect(page.getByTestId("batch-price-error")).toContainText("The price per kg must be at least 1.")
    await expect(page.getByTestId("batch-kg-h1-error")).toContainText("current, verified actual figure")
    await expect(page.getByTestId("batch-made")).toHaveCount(0)
    await expectNoHorizontalScroll(page)
  })

  test("403 four eyes: the verifier is told someone else must buy it, as the API wrote it", async ({ page }) => {
    await buying(page, [ownVerification as Reply])
    await buy(page)
    await expect(page.getByTestId("batch-error")).toContainText("A harvest you verified must be bought by someone else.")
    await expect(page.getByTestId("batch-made")).toHaveCount(0)
  })

  test("a lost reply is retried under the same client_ref, and the replay is the one batch", async ({ page }) => {
    const sent = await buying(page, [{ status: 503, body: unavailable }, batchMade as Reply])
    await buy(page)
    await expect(page.getByTestId("batch-error")).toBeVisible()
    await page.getByTestId("batch-submit").click()
    await page.getByTestId("confirm-dialog-confirm").click()
    await expect(page.getByTestId("batch-made")).toContainText("ILU-C-01")
    expect(sent).toHaveLength(2)
    expect(sent[0]!.client_ref).toMatch(KEY)
    expect(sent[1]).toEqual(sent[0])
  })
})

test.describe("options that fail to load", () => {
  test.use({ allow: { responses: [{ url: "/v1/workspace/w1/farm/supply-batches/options?village_id=v1&crop_id=crop1", status: 503 }] } })

  test("show the error with a retry, not an empty card", async ({ page }) => {
    await buying(page, [batchMade as Reply], { status: 503, body: unavailable })
    await expect(page.getByRole("alert")).toBeVisible()
    await expect(page.getByTestId("batch-no-harvests")).toHaveCount(0)
  })
})
