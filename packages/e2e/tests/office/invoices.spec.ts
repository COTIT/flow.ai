import { expectNoRawValues } from "../../src/test"

import { expect, expectFits, type FakeApi, paged, test } from "./fake-api"

const invoice = {
  id: "inv-1",
  invoice_nr: "2026-0001",
  customer_name: "Acme Imports",
  status: "partially_paid",
  is_credit: false,
  created_at: "2026-10-30T09:00:00Z",
  due_date: "2026-11-13T00:00:00Z",
  total_amount: "121.00",
  allowed_actions: [],
  balance: { open: 21, paid: "100.00000000", total: 121 },
  items: [
    {
      id: "it-1",
      title: "AFM Honey 500g",
      quantity: "10.00",
      price_incl: "12.10",
      total_incl: "121.00",
    },
  ],
  relationships: {
    currency: { asset_type: "EUR", decimals: 2 },
    payments: [
      {
        id: "pay-1",
        date: "2026-10-31",
        payment_method: "bank",
        amount: "100.00000000",
      },
    ],
  },
}

function invoicesApi(api: FakeApi) {
  api
    .on("GET", "/workspace/:ws/invoices", () =>
      paged([
        invoice,
        {
          ...invoice,
          id: "inv-2",
          invoice_nr: "2026-0002",
          customer_name: null,
        },
      ])
    )
    .on("GET", "/workspace/:ws/invoices/:id", () => ({ data: invoice }))
    .on("GET", "/workspace/:ws/invoices/:id/preview", () => ({
      data: {
        id: "file-1",
        file_path: "https://files.example.com/inv.pdf?sig=x",
      },
    }))
}

test("invoice list to detail; the API's amounts in the invoice's currency", async ({
  page,
  api,
}) => {
  invoicesApi(api)
  await page.goto("/invoices")
  await expect(page.getByRole("cell", { name: "€121.00" }).first()).toBeVisible()
  await expect(
    page
      .getByRole("row", { name: /2026-0001/ })
      .getByRole("cell", { name: "Acme Imports" })
  ).toBeVisible()
  // No customer on the API's side: a dash, not a guess.
  await expect(
    page
      .getByRole("row", { name: /2026-0002/ })
      .getByRole("cell", { name: "—" })
      .first()
  ).toBeVisible()
  await page.getByRole("link", { name: "2026-0001" }).click()
  await expect(page.getByRole("heading", { name: "2026-0001" })).toBeVisible()
  await expect(page.getByRole("cell", { name: "AFM Honey 500g" })).toBeVisible()
  await expect(page.getByRole("cell", { name: "€100.00" })).toBeVisible()
  await expect(page.getByRole("region", { name: "Lines" })).toContainText(
    "Amounts as calculated by the server."
  )
  await expect(
    page
      .getByRole("region", { name: "Payments" })
      .getByRole("cell", { name: "bank" })
  ).toBeVisible()
  expect(
    api.callsTo("GET", "/workspace/:ws/invoices/:id/preview")
  ).toHaveLength(0)
  await expectFits(page)
})

test("PDF is made only on request", async ({ page, api }) => {
  invoicesApi(api)
  await page.goto("/invoices/inv-1")
  await page.getByRole("button", { name: "Make PDF" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "The PDF is ready." })
  ).toBeVisible()
  await expect(page.getByRole("link", { name: "Open PDF" })).toHaveAttribute(
    "href",
    "https://files.example.com/inv.pdf?sig=x"
  )
  expect(
    api.callsTo("GET", "/workspace/:ws/invoices/:id/preview")
  ).toHaveLength(1)
})

test.describe(() => {
  test.use({
    allow: { responses: [{ url: "/v1/workspace/w1/invoices/inv-1/preview", status: 500 }] },
  })

  test("/invoices/[id] PDF: a failure says so and no link is offered", async ({ page, api }) => {
    invoicesApi(api)
    api.on("GET", "/workspace/:ws/invoices/:id/preview", () => ({
      status: 500,
      body: { message: "The PDF could not be made." },
    }))
    await page.goto("/invoices/inv-1")
    await page.getByRole("button", { name: "Make PDF" }).click()
    await expect(
      page.getByRole("alert").filter({ hasText: "The PDF could not be made." })
    ).toBeVisible()
    await expect(page.getByRole("link", { name: "Open PDF" })).toHaveCount(0)
    // No silent retries of a PDF the server just failed to make.
    expect(api.callsTo("GET", "/workspace/:ws/invoices/:id/preview")).toHaveLength(1)
  })

  test("/invoices/[id] PDF: after a failure, Make PDF asks the API again", async ({ page, api }) => {
    invoicesApi(api)
    let fail = true
    api.on("GET", "/workspace/:ws/invoices/:id/preview", () =>
      fail
        ? { status: 500, body: { message: "The PDF could not be made." } }
        : { data: { id: "file-1", file_path: "https://files.example.com/inv.pdf?sig=x" } }
    )
    await page.goto("/invoices/inv-1")
    await page.getByRole("button", { name: "Make PDF" }).click()
    await expect(
      page.getByRole("alert").filter({ hasText: "The PDF could not be made." })
    ).toBeVisible()
    fail = false
    await page.getByRole("button", { name: "Make PDF" }).click()
    await expect(page.getByRole("link", { name: "Open PDF" })).toBeVisible({ timeout: 5_000 })
  })
})

const eur = { asset_type: "EUR", decimals: 2 }

test("list: every number, status and amount is readable", async ({ page, api }) => {
  const row = (over: Record<string, unknown>) => ({
    ...invoice,
    relationships: { currency: eur },
    ...over,
  })
  api.on("GET", "/workspace/:ws/invoices", () =>
    paged([
      row({ id: "d", invoice_nr: null, status: "draft", customer_name: "Old Mill Gift Shop" }),
      row({
        id: "c",
        invoice_nr: "N/A",
        status: "cancelled",
        customer_name: "Two Ravens Coffee Bar",
        total_amount: "449.40",
        balance: { open: 449.4, paid: 0 },
      }),
      row({ id: "p", invoice_nr: "1", status: "pending", customer_name: "Brightwater Co-op" }),
      row({
        id: "n",
        invoice_nr: "2",
        status: "credited",
        is_credit: true,
        customer_name: "Copper Kettle",
        total_amount: "-12.05",
        balance: { open: 0, paid: "-12.05000000" },
      }),
    ])
  )
  await page.goto("/invoices")
  const draft = page.getByRole("row", { name: /Old Mill/ })
  await expect(draft.getByRole("cell").first()).toHaveText("—")
  await expect(draft.getByRole("link", { name: "Open the invoice without a number" })).toBeVisible()
  const cancelled = page.getByRole("row", { name: /Two Ravens/ })
  await expect(cancelled.getByRole("cell").first()).toHaveText("—")
  // Total and open in one format, from the API's text and its JSON number alike.
  await expect(cancelled.getByRole("cell", { name: "€449.40" }).first()).toBeVisible()
  // Open is hidden on a phone, so count the cells' text.
  const texts = await cancelled.getByRole("cell", { includeHidden: true }).allTextContents()
  expect(texts.filter((x) => x === "€449.40")).toHaveLength(2)
  await expect(page.getByRole("row", { name: /Brightwater/ })).toContainText("Sent")
  await expect(page.getByRole("row", { name: /Copper Kettle/ })).toContainText("-€12.05")
  await expect(page.getByText("N/A")).toHaveCount(0)
  await expectNoRawValues(page, { allow: [/\b\d{4}-\d{4}\b/] })
  await expectFits(page)
})

test("detail: a draft and a credit note get their own title", async ({ page, api }) => {
  let current: Record<string, unknown> = { ...invoice, invoice_nr: null, status: "draft" }
  api.on("GET", "/workspace/:ws/invoices/:id", () => ({ data: current }))
  await page.goto("/invoices/inv-1")
  await expect(page.getByRole("heading", { name: "Draft invoice" })).toBeVisible()
  current = { ...invoice, id: "inv-2", invoice_nr: "2026-0008", is_credit: true, status: "pending" }
  await page.goto("/invoices/inv-2")
  await expect(page.getByRole("heading", { name: "Credit note 2026-0008" })).toBeVisible()
  await expectNoRawValues(page, { allow: [/\b\d{4}-\d{4}\b/] })
})

test("nl: statuses and the draft title in Dutch", async ({ page, api, context, baseURL }) => {
  await context.addCookies([{ name: "NEXT_LOCALE", value: "nl", url: baseURL! }])
  api.on("GET", "/workspace/:ws/invoices", () =>
    paged([{ ...invoice, status: "pending", relationships: { currency: eur } }])
  )
  await page.goto("/invoices")
  await expect(page.getByRole("row", { name: /2026-0001/ })).toContainText("Verzonden")
  await expect(page.getByRole("row", { name: /2026-0001/ })).toContainText("€ 121,00")
  await expectNoRawValues(page, { allow: [/\b\d{4}-\d{4}\b/] })
})
