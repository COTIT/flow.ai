import { expect, paged, test, type FakeApi } from "./fake-api"

const invoice = {
  id: "inv-1",
  invoice_nr: "2026-0001",
  customer_name: "Acme Imports",
  status: "sent",
  is_credit: false,
  created_at: "2026-10-30T09:00:00Z",
  due_date: "2026-11-13T00:00:00Z",
  total_amount: "121.00",
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

test("invoice list to detail; amounts shown as the API gives them", async ({
  page,
  api,
}) => {
  invoicesApi(api)
  await page.goto("/invoices")
  await expect(page.getByRole("cell", { name: "121.00" }).first()).toBeVisible()
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
  await expect(page.getByRole("cell", { name: "100.00000000" })).toBeVisible()
  expect(
    api.callsTo("GET", "/workspace/:ws/invoices/:id/preview")
  ).toHaveLength(0)
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
