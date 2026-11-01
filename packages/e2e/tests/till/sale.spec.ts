import { expectNoHorizontalScroll } from "../../src/test"
import {
  catalogPage,
  expect,
  honey,
  seedTill,
  serveCatalog,
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

/** The API's answer: a total of €33.02 rounds to €33.00 in cash. */
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
      },
    },
  }
}

function saleApi(api: FakeApi) {
  serveCatalog(api, [honey, candle])
  api.on("POST", "/workspace/:ws/till/sales", ({ body }) =>
    booked(body as Parameters<typeof booked>[0])
  )
}

async function ringUp(page: import("@playwright/test").Page) {
  await page.getByLabel("Search products").fill("honey")
  await page.getByRole("button", { name: /Ilundo honey 500 g/ }).click()
  await page
    .getByRole("button", { name: "One more Ilundo honey 500 g" })
    .click()
  await page.getByLabel("Scan or type a barcode").fill("8712345000028")
  await page.getByLabel("Scan or type a barcode").press("Enter")
}

test("a cash sale sends the catalog prices and shows the API's rounding and change", async ({
  page,
  api,
}) => {
  saleApi(api)
  await seedTill(page)
  await page.goto("/sale")
  await ringUp(page)

  const cart = page.getByRole("region", { name: "Cart" })
  await expect(cart.getByLabel("Quantity of Ilundo honey 500 g")).toHaveText(
    "2"
  )
  await expect(cart.getByText("Beeswax candle · Large")).toBeVisible()
  await expect(cart.getByText("≈ €29.55")).toBeVisible()
  await expectNoHorizontalScroll(page)

  await page.getByLabel("Cash received").fill("50")
  await page.getByRole("button", { name: "Charge" }).click()

  const receipt = page.getByRole("region", { name: "Receipt" })
  await expect(
    receipt.getByRole("heading", { name: "Receipt 1042" })
  ).toBeVisible()
  await expect(receipt.getByText("€33.02")).toBeVisible()
  await expect(receipt.getByText("-€0.02")).toBeVisible()
  await expect(receipt.getByText("€33.00")).toBeVisible()
  await expect(receipt.getByText("€17.00")).toBeVisible()
  await expectNoHorizontalScroll(page)

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

  await page.getByRole("button", { name: "New sale" }).click()
  await expect(page.getByText("The cart is empty.")).toBeVisible()
})

test("a card sale records the slip reference and has no change", async ({
  page,
  api,
}) => {
  saleApi(api)
  await seedTill(page)
  await page.goto("/sale")
  await ringUp(page)
  await page.getByRole("button", { name: "Card" }).click()
  await page.getByLabel("Card slip reference").fill("SLIP-77")
  await page.getByRole("button", { name: "Charge" }).click()

  const receipt = page.getByRole("region", { name: "Receipt" })
  await expect(receipt.getByText("SLIP-77")).toBeVisible()
  await expect(receipt.getByText("Change")).toHaveCount(0)
  const [post] = api.callsTo("POST", `/workspace/${WS}/till/sales`)
  expect((post!.body as Record<string, unknown>).payment).toEqual({
    method: "card",
    reference: "SLIP-77",
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
  await page.getByLabel("Scan or type a barcode").fill("8712345000028")
  await page.getByLabel("Scan or type a barcode").press("Enter")
  await expect(page.getByText("Beeswax candle · Large")).toBeVisible()
  expect(asked).toContain("8712345000028")
})

test("an unknown barcode is reported, nothing added", async ({ page, api }) => {
  saleApi(api)
  await seedTill(page)
  await page.goto("/sale")
  await page.getByLabel("Scan or type a barcode").fill("123")
  await page.getByLabel("Scan or type a barcode").press("Enter")
  await expect(
    page.getByRole("alert").filter({ hasText: "No product with code 123." })
  ).toBeVisible()
  await expect(page.getByText("The cart is empty.")).toBeVisible()
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
    await page.getByLabel("Cash received").fill("10")
    await page.getByRole("button", { name: "Charge" }).click()
    await expect(
      page
        .getByRole("alert")
        .filter({ hasText: "The cash tendered is less than due." })
    ).toBeVisible()

    await page.getByLabel("Cash received").fill("40")
    await page.getByRole("button", { name: "Charge" }).click()
    await expect(
      page.getByRole("heading", { name: "Receipt 1042" })
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
    await page.getByLabel("Cash received").fill("50")
    await page.getByRole("button", { name: "Charge" }).click()
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
