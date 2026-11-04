import type { Page } from "@playwright/test"

import { expectTillLook, tapKeys } from "./look"
import {
  catalogPage,
  expect,
  honey,
  seedTill,
  quoteReply,
  serveCatalog,
  serveQuote,
  test,
  type FakeApi,
  type TillRow,
  WS,
} from "./fake-till"

const candle: TillRow = {
  ...honey,
  id: "p-candle",
  name: "Beeswax candle",
  ean13: null,
  unit_price: 496,
  variants: [
    {
      id: "v-small",
      sku: "C-S",
      barcode: null,
      ean13: "8712345000011",
      is_default: true,
      unit_price: 496,
      options: [{ type: "Size", value: "Small" }],
    },
    {
      id: "v-large",
      sku: "C-L",
      barcode: null,
      ean13: "8712345000028",
      is_default: false,
      unit_price: 661,
      options: [{ type: "Size", value: "Large" }],
    },
  ],
}

const salesUrl = `/v1/workspace/${WS}/till/sales`

/**
 * The API's answer: a total of €33.02 rounds to €33.00 in cash. Its booked lines carry the
 * server's titles and totals, which the receipt shows rather than the basket's.
 */
function booked(body: {
  client_ref: string
  payment: Record<string, unknown>
}) {
  const cash = body.payment.method === "cash"
  return {
    status: 201,
    body: {
      data: {
        order_id: "o-1",
        number: "1042",
        client_ref: body.client_ref,
        register_id: "reg-1",
        till_user_id: "tu-1",
        method: body.payment.method,
        total: 3302,
        server_total: 3302,
        price_mismatch: 0,
        mismatched_lines: 0,
        due: cash ? 3300 : 3302,
        rounding: cash ? -2 : 0,
        tendered: cash ? body.payment.tendered : null,
        change: cash ? (body.payment.tendered as number) - 3300 : null,
        card_reference: cash ? null : body.payment.reference,
        sold_at: "2026-11-01T09:12:00Z",
        lines: [
          {
            title: "Wildflower honey 500 g (lot 7)",
            product_id: "p-honey",
            product_variant_id: null,
            quantity: "2.00",
            unit_price: 1147,
            line_excl: 2294,
            line_vat: 206,
            line_total: 2500,
          },
          {
            title: "Beeswax candle · Large",
            product_id: "p-candle",
            product_variant_id: "v-large",
            quantity: "1.00",
            unit_price: 661,
            line_excl: 661,
            line_vat: 141,
            line_total: 802,
          },
        ],
      },
    },
  }
}

function saleApi(api: FakeApi) {
  serveCatalog(api, [honey, candle])
  serveQuote(api, [honey, candle])
  api.on("POST", "/workspace/:ws/till/sales", ({ body }) =>
    booked(body as Parameters<typeof booked>[0])
  )
}

const scan = (page: Page) => page.getByLabel("Scan a barcode or type a name")

/** Two honeys (tile, then +) and a large candle (scanned). */
async function ringUp(page: Page) {
  await page
    .getByRole("list", { name: "Products" })
    .getByRole("button", { name: /Ilundo honey 500 g/ })
    .click()
  await page
    .getByRole("button", { name: "One more Ilundo honey 500 g" })
    .click()
  await scan(page).fill("8712345000028")
  await scan(page).press("Enter")
}

/** Opens the payment sheet, card or cash. */
async function charge(page: Page, method: "Cash" | "Card" = "Cash") {
  await page.getByRole("button", { name: /^Charge/ }).click()
  const sheet = page.getByRole("dialog", { name: "Take payment" })
  await sheet.getByRole("button", { name: method }).click()
  return sheet
}

test("a cash sale sends the catalog prices and shows the API's rounding and change", async ({
  page,
  api,
}) => {
  saleApi(api)
  // The API's quote: €33.02, which cash pays as €33.00 (5-cent rounding).
  api.on("POST", "/workspace/:ws/till/sales/quote", ({ body }) => {
    const b = body as Parameters<typeof quoteReply>[0]
    return quoteReply(b, 3302, b.payment.method === "cash" ? 3300 : 3302)
  })
  await seedTill(page)
  await page.goto("/sale")
  await expect(page.getByText("The basket is empty.")).toBeVisible()
  await expectTillLook(page)
  await ringUp(page)

  const basket = page.getByRole("complementary", { name: "Basket" })
  await expect(basket.getByText("Basket · 3 items")).toBeVisible()
  await expect(basket.getByLabel("Quantity of Ilundo honey 500 g")).toHaveText(
    "2"
  )
  await expect(basket.getByText("Beeswax candle · Large")).toBeVisible()
  // The tile shows how many are in the basket.
  await expect(
    page.getByRole("button", { name: /Ilundo honey 500 g.*2 in the basket/ })
  ).toBeVisible()
  const quote = basket.getByRole("definition")
  await expect(quote.filter({ hasText: "€33.02" })).toBeVisible()
  await expect(quote.filter({ hasText: "-€0.02" })).toBeVisible()
  await expect(quote.filter({ hasText: "€33.00" })).toBeVisible()
  await expect(basket.getByRole("button", { name: /^Charge/ })).toHaveText(
    "Charge€33.00"
  )
  await expect(basket.getByText("≈")).toHaveCount(0)
  await expectTillLook(page)

  const sheet = await charge(page)
  await expect(sheet.getByText("€33.00").first()).toBeVisible()
  // Exact, then the smallest accepted notes that cover what is due (only €50 here).
  await expect(sheet.locator(".notes").getByRole("button")).toHaveText([
    "Exact",
    "€50.00",
  ])
  await sheet.getByRole("button", { name: "Exact" }).click()
  await expect(sheet.getByLabel("Received")).toHaveText("€33.00")
  await sheet.getByRole("button", { name: "Delete" }).click()
  await expect(sheet.getByLabel("Received")).toHaveText("€3.30")
  await sheet.getByRole("button", { name: "€50.00" }).click()
  await expect(sheet.getByLabel("Received")).toHaveText("€50.00")
  await expect(sheet.getByLabel("Change")).toHaveText("€17.00")
  await expectTillLook(page)
  const quoted = api
    .callsTo("POST", `/workspace/${WS}/till/sales/quote`)
    .at(-1)!
  expect(quoted.body).toEqual({
    lines: [
      { product_id: "p-honey", quantity: "2", unit_price: 1147 },
      {
        product_id: "p-candle",
        product_variant_id: "v-large",
        quantity: "1",
        unit_price: 661,
      },
    ],
    payment: { method: "cash", tendered: 5000 },
  })
  await sheet.getByRole("button", { name: /^Book sale/ }).click()

  const receipt = page.getByRole("dialog", { name: "Done" })
  await expect(
    receipt.getByRole("status").filter({ hasText: "Sale 1042 booked" })
  ).toBeVisible()
  // The booked lines as the API answered them, not the basket's names or the quote's totals.
  await expect(
    receipt.getByText("2 × Wildflower honey 500 g (lot 7)")
  ).toBeVisible()
  await expect(receipt.getByText("€25.00")).toBeVisible()
  await expect(receipt.getByText("1 × Beeswax candle · Large")).toBeVisible()
  await expect(receipt.getByText("€8.02")).toBeVisible()
  await expect(receipt.getByText("Ilundo honey 500 g")).toHaveCount(0)
  await expect(receipt.getByText("€33.02")).toBeVisible()
  await expect(receipt.getByText("-€0.02")).toBeVisible()
  await expect(receipt.getByText("€33.00")).toBeVisible()
  await expect(receipt.getByText("€50.00")).toBeVisible()
  await expect(receipt.getByLabel("Give change")).toHaveText("€17.00")
  await expectTillLook(page)

  const [post] = api.callsTo("POST", `/workspace/${WS}/till/sales`)
  const body = post!.body as Record<string, unknown>
  expect(body.client_ref).toMatch(/^[0-9a-f-]{36}$/)
  expect(body.lines).toEqual([
    { product_id: "p-honey", quantity: "2", unit_price: 1147 },
    {
      product_id: "p-candle",
      product_variant_id: "v-large",
      quantity: "1",
      unit_price: 661,
    },
  ])
  expect(body.payment).toEqual({ method: "cash", tendered: 5000 })

  await receipt.getByRole("button", { name: "New sale" }).click()
  await expect(page.getByRole("dialog")).toHaveCount(0)
  await expect(page.getByText("The basket is empty.")).toBeVisible()
})

test("a card sale records the slip reference and has no change", async ({
  page,
  api,
}) => {
  saleApi(api)
  await seedTill(page)
  await page.goto("/sale")
  await ringUp(page)
  // Line totals are the quote's (2 × €12.50, and the candle at €12.50).
  const basket = page.getByRole("complementary", { name: "Basket" })
  await expect(basket.locator(".lp")).toHaveText(["€25.00", "€12.50"])
  const sheet = await charge(page, "Card")
  await tapKeys(page, "48210")
  await sheet.getByRole("button", { name: "Delete" }).click()
  await expect(sheet.getByLabel("Card slip or last 4 digits")).toHaveText(
    "4821"
  )
  await expectTillLook(page)
  await sheet.getByRole("button", { name: /^Book card sale/ }).click()

  const receipt = page.getByRole("dialog", { name: "Done" })
  await expect(receipt.getByText("4821")).toBeVisible()
  await expect(receipt.getByText("Give change")).toHaveCount(0)
  const [post] = api.callsTo("POST", `/workspace/${WS}/till/sales`)
  expect((post!.body as Record<string, unknown>).payment).toEqual({
    method: "card",
    reference: "4821",
  })
})

test("a scan asks the API for the exact code", async ({ page, api }) => {
  const asked: string[] = []
  saleApi(api)
  api.on("GET", "/workspace/:ws/till/catalog", ({ query }) => {
    asked.push(query.get("code") ?? "")
    return catalogPage([candle])
  })
  await seedTill(page)
  await page.goto("/sale")
  await scan(page).fill("8712345000028")
  await scan(page).press("Enter")
  await expect(
    page
      .getByRole("complementary", { name: "Basket" })
      .getByText("Beeswax candle · Large")
  ).toBeVisible()
  await expect(scan(page)).toHaveValue("")
  expect(asked).toContain("8712345000028")
})

test("an unknown barcode is reported, nothing added", async ({ page, api }) => {
  saleApi(api)
  await seedTill(page)
  await page.goto("/sale")
  await scan(page).fill("123")
  await scan(page).press("Enter")
  await expect(
    page.getByRole("alert").filter({ hasText: "No product with code 123." })
  ).toBeVisible()
  await expect(page.getByText("The basket is empty.")).toBeVisible()
})

test.describe("refused sales", () => {
  test.use({
    allow: {
      responses: [
        { url: salesUrl, status: 422 },
        { url: salesUrl, status: 409 },
      ],
    },
  })

  test("too little cash shows the API's message; the retry keeps the sale's client_ref", async ({
    page,
    api,
  }) => {
    saleApi(api)
    let first = true
    api.on("POST", "/workspace/:ws/till/sales", ({ body }) => {
      if (!first) return booked(body as Parameters<typeof booked>[0])
      first = false
      return {
        status: 422,
        body: {
          message: "The given data was invalid.",
          errors: {
            "payment.tendered": ["The cash tendered is less than due."],
          },
        },
      }
    })
    await seedTill(page)
    await page.goto("/sale")
    await ringUp(page)
    const sheet = await charge(page)
    await tapKeys(page, "1000")
    await sheet.getByRole("button", { name: /^Book sale/ }).click()
    await expect(
      sheet
        .getByRole("alert")
        .filter({ hasText: "The cash tendered is less than due." })
    ).toBeVisible()
    await expectTillLook(page, { shift: false })

    for (let i = 0; i < 4; i++)
      await sheet.getByRole("button", { name: "Delete" }).click()
    await tapKeys(page, "4000")
    await expect(sheet.getByLabel("Received")).toHaveText("€40.00")
    await sheet.getByRole("button", { name: /^Book sale/ }).click()
    await expect(
      page.getByRole("dialog", { name: "Done" }).getByText("Sale 1042 booked")
    ).toBeVisible()
    const posts = api.callsTo("POST", `/workspace/${WS}/till/sales`)
    expect(posts).toHaveLength(2)
    const refs = posts.map((p) => (p.body as { client_ref: string }).client_ref)
    expect(refs[0]).toBe(refs[1])
  })

  test("no open shift points to the drawer", async ({ page, api }) => {
    saleApi(api)
    api.on("POST", "/workspace/:ws/till/sales", () => ({
      status: 409,
      body: { message: "No shift is open on this register." },
    }))
    await seedTill(page)
    await page.goto("/sale")
    await ringUp(page)
    const sheet = await charge(page)
    await sheet.getByRole("button", { name: "€50.00" }).click()
    await sheet.getByRole("button", { name: /^Book sale/ }).click()
    await expect(
      page
        .getByRole("alert")
        .filter({ hasText: "No shift is open on this register." })
    ).toBeVisible()
    await expect(
      page.getByRole("link", { name: "Open the drawer" })
    ).toBeVisible()
  })
})
