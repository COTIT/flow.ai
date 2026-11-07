import { expectNoRawValues } from "../../src/test"

import { expect, expectFits, type FakeApi, paged, test, WS } from "./fake-api"

// The invoice's Next step card offers exactly the API's `allowed_actions`; each dialog sends
// the body its route takes and shows the outcome in words.
const allow = [/\b\d{4}-\d{4}\b/, /\bORD-\d{4}\b/]
const eur = { asset_type: "EUR", decimals: 2 }
const relation = {
  id: "rel-1",
  company_name: "Old Mill Gift Shop",
  addresses: [{ address: "Mill Lane", house_number: "4", zipcode: "1234AB", city: "Utrecht", country: "NL", type: "billing" }],
  relationships: {
    contacts: [{ id: "con-1", full_name: "Ada Miller", email: "ada@oldmill.test", default_contact: true }],
  },
}

type Invoice = Record<string, unknown> & {
  status: string
  allowed_actions: string[]
  invoice_nr: string | null
}

function invoice(over: Partial<Invoice> = {}): Invoice {
  return {
    id: "inv-1",
    invoice_nr: null,
    status: "draft",
    is_credit: false,
    reference: null,
    note: null,
    created_at: "2026-11-05T09:00:00Z",
    due_date: null,
    total_amount: "121.00",
    balance: { open: 121, paid: 0, total: 121 },
    money: { currency: "EUR", decimals: 2, subtotal: 10000, vat: 2100, total: 12100, paid: 0, open: 12100 },
    billing_address: { address: "Mill Lane", house_number: "4", zipcode: "1234AB", city: "Utrecht", country: "NL" },
    addressed_contact: null,
    items: [
      { id: "it-1", title: "AFM Honey 500g", quantity: "10.00", price_incl: "12.10", total_incl: "121.00", unit_price_minor: 1210, total_minor: 12100, amount_left_to_credit: "10.00" },
    ],
    allowed_actions: ["edit", "send", "cancel", "duplicate"],
    relationships: { currency: eur, relation, payments: [], order: null },
    ...over,
  }
}

/** Serves one invoice that the action routes change, like the API would. */
function invoiceApi(api: FakeApi, start: Invoice) {
  const state = { inv: start }
  api
    .on("GET", "/workspace/:ws/invoices/:id", () => ({ data: state.inv }))
    .on("GET", "/workspace/:ws/invoices", () => paged([]))
    .on("POST", "/workspace/:ws/invoices/:id/send", ({ body }) => {
      state.inv = { ...state.inv, status: "pending", invoice_nr: "2026-0007", allowed_actions: ["resend", "duplicate", "pay"] }
      return deliveryReceipt((body as { send_method?: string }).send_method === "manual" ? "manual" : "accepted")
    })
    .on("POST", "/workspace/:ws/invoices/:id/resend", () => deliveryReceipt("accepted"))
  return state
}

const deliveryReceipt = (status: string) => ({
  message: "ok",
  delivery: { status, invoice_delivery_id: "delivery-1", invoice_id: "inv-1", send_log_id: null, pdf_file_id: null },
})

const deliveryNotices = [
  ["queued", "Invoice numbered. Email delivery is queued.", "info"],
  ["accepted", "Invoice numbered. The mail server accepted the email; delivery is unconfirmed.", "info"],
  ["manual", "Invoice numbered. Deliver it to the customer manually; no email was sent.", "info"],
  ["blocked", "Invoice numbered. Email delivery is blocked; check the mail settings.", "warning"],
  ["failed", "Invoice numbered. Email delivery failed; check its status before trying again.", "warning"],
  ["unknown", "Invoice numbered. Email delivery is uncertain; verify its status before trying again.", "warning"],
  ["legacy", "The invoice may have changed. Delivery is unconfirmed; check its status before trying again.", "warning"],
] as const

for (const action of ["send", "resend"] as const) {
  for (const [status, message, variant] of deliveryNotices) {
    test(`invoice ${action} delivery receipt: ${status}`, async ({ page, api }) => {
      const state = invoiceApi(api, invoice(action === "resend"
        ? { status: "pending", invoice_nr: "2026-0007", allowed_actions: ["resend", "duplicate", "pay"] }
        : {}))
      api.on("POST", `/workspace/:ws/invoices/:id/${action}`, () => {
        state.inv = { ...state.inv, status: "pending", invoice_nr: "2026-0007", allowed_actions: ["resend", "duplicate", "pay"] }
        return status === "legacy" ? { message: "ok" } : deliveryReceipt(status)
      })
      await page.goto("/invoices/inv-1")
      const label = action === "resend" ? "Send again" : "Send"
      await page.getByRole("button", { name: label, exact: true }).click()
      const dialog = page.getByRole("dialog")
      if (action === "send" && status !== "manual")
        await dialog.getByRole("radio", { name: "Email to the customer" }).check()
      await dialog.getByRole("button", { name: label, exact: true }).click()
      await expect(dialog).toHaveCount(0)
      await expect(page.getByRole("heading", { name: "Invoice 2026-0007" })).toBeVisible()
      const notice = page.getByRole("status").filter({ hasText: message })
      await expect(notice).toBeVisible()
      await expect(notice.locator('[data-slot="notice"]')).toHaveAttribute("data-variant", variant)
      await expect(page.getByText(/^Invoice sent(?: again)?\.$/)).toHaveCount(0)
      expect(api.callsTo("POST", `/workspace/:ws/invoices/:id/${action}`)).toHaveLength(1)
      await expectFits(page)
    })
  }
}

test("draft: number it for manual delivery without claiming email was sent", async ({ page, api }) => {
  invoiceApi(api, invoice())
  await page.goto("/invoices/inv-1")
  const next = page.getByRole("region", { name: "Next step" })
  await expect(next.getByText("A draft gets its number when you send it.")).toBeVisible()
  for (const name of ["Send", "Edit details", "Cancel draft", "Duplicate"])
    await expect(next.getByRole("button", { name, exact: true })).toBeVisible()
  await expect(next.getByRole("button", { name: "Record payment" })).toHaveCount(0)

  await next.getByRole("button", { name: "Send", exact: true }).click()
  const dialog = page.getByRole("dialog", { name: "Send the invoice" })
  await expect(dialog.getByRole("radio", { name: "Number for manual delivery (no email)" })).toBeChecked()
  await expectFits(page)
  await dialog.getByRole("button", { name: "Send", exact: true }).click()
  await expect(page.getByText("Invoice numbered. Deliver it to the customer manually; no email was sent.")).toBeVisible()
  await expect(page.getByRole("heading", { name: "Invoice 2026-0007" })).toBeVisible()
  await expect(next.getByRole("button", { name: "Record payment" })).toBeVisible()
  const [call] = api.callsTo("POST", "/workspace/:ws/invoices/:id/send")
  expect(call!.body).toMatchObject({
    send_method: "manual",
    addressed_contact_id: "con-1",
    address: { address: "Mill Lane", city: "Utrecht", country: "NL" },
  })
  await expectNoRawValues(page, { allow })
})

test.describe(() => {
  test.use({ allow: { responses: [{ url: "/v1/workspace/w1/invoices/inv-1/send", status: 503 }, { url: "/v1/workspace/w1/invoices/inv-1/preview", status: 503 }] } })

  test("email and PDF without a server set up say so in words", async ({ page, api }) => {
    invoiceApi(api, invoice())
    api
      .on("POST", "/workspace/:ws/invoices/:id/send", () => ({ status: 503, body: { message: "PDF rendering isn't configured." } }))
      .on("GET", "/workspace/:ws/invoices/:id/preview", () => ({ status: 503, body: { message: "File storage isn't configured." } }))
    await page.goto("/invoices/inv-1")
    await page.getByRole("button", { name: "Send", exact: true }).click()
    const dialog = page.getByRole("dialog", { name: "Send the invoice" })
    await dialog.getByRole("radio", { name: "Email to the customer" }).check()
    await expect(dialog.getByLabel("To", { exact: true })).toHaveValue("ada@oldmill.test")
    await dialog.getByRole("button", { name: "Send", exact: true }).click()
    await expect(dialog.getByText("Email sending isn't set up yet.")).toBeVisible()
    await expect(page.getByText("PDF rendering isn't configured.")).toHaveCount(0)
    await dialog.getByRole("button", { name: "Cancel" }).click()
    await page.getByRole("button", { name: "Make PDF" }).click()
    await expect(page.getByText("PDFs aren't available on this server yet.")).toBeVisible()
    await expect(page.getByText("File storage isn't configured.")).toHaveCount(0)
  })
})

test("a customer without a contact person can't be sent to: the card says why", async ({ page, api }) => {
  invoiceApi(api, invoice({ relationships: { currency: eur, relation: { ...relation, relationships: { contacts: [] } }, payments: [], order: null } }))
  await page.goto("/invoices/inv-1")
  await page.getByRole("button", { name: "Send", exact: true }).click()
  const dialog = page.getByRole("dialog", { name: "Send the invoice" })
  await expect(dialog.getByText("This customer has no contact person yet.")).toBeVisible()
  await expect(dialog.getByRole("button", { name: "Send", exact: true })).toBeDisabled()
})

test("pay: part, then a credit note settles the rest", async ({ page, api }) => {
  const seen: string[] = []
  const state = invoiceApi(
    api,
    invoice({ status: "pending", invoice_nr: "2026-0007", allowed_actions: ["resend", "duplicate", "pay"] })
  )
  api
    .on("POST", "/workspace/:ws/invoices/:id/payments", () => {
      state.inv = { ...state.inv, status: "partially_paid", money: { currency: "EUR", decimals: 2, subtotal: 10000, vat: 2100, total: 12100, paid: 10000, open: 2100 } }
      return { status: 201, body: { data: { invoice_payment_id: "pay-1" } } }
    })
    .on("GET", "/workspace/:ws/invoices", ({ query }) => {
      seen.push(query.toString())
      return paged([{ id: "cn-1", invoice_nr: "2026-0009", is_credit: true, status: "pending", money: { currency: "EUR", decimals: 2, subtotal: -1736, vat: -364, total: -2100, paid: 0, open: -2100 }, relationships: { currency: eur } }])
    })
  await page.goto("/invoices/inv-1")
  await page.getByRole("button", { name: "Record payment" }).click()
  const dialog = page.getByRole("dialog", { name: "Record a payment" })
  const amount = dialog.getByLabel("Amount (EUR)")
  // No guess and no sums: the field starts empty, the API's open amount is the hint.
  await expect(amount).toHaveValue("")
  await expect(dialog.getByText("Open: €121.00")).toBeVisible()
  await expect(dialog.getByRole("button", { name: "Record payment" })).toBeDisabled()
  await amount.fill("100")
  await dialog.getByLabel("Paid with").selectOption({ label: "Cash" })
  await expectFits(page)
  await dialog.getByRole("button", { name: "Record payment" }).click()
  await expect(page.getByText("Payment recorded.")).toBeVisible()
  await expect(page.getByRole("banner").or(page.locator("main")).getByText("Partly paid").first()).toBeVisible()
  expect(api.callsTo("POST", "/workspace/:ws/invoices/:id/payments")[0]!.body).toMatchObject({ amount: "100", payment_method: "cash" })
  expect(api.callsTo("POST", "/workspace/:ws/invoices/:id/payments")[0]!.headers["idempotency-key"]).toBeTruthy()

  await page.getByRole("button", { name: "Record payment" }).click()
  const again = page.getByRole("dialog", { name: "Record a payment" })
  await expect(again.getByText("Open: €21.00")).toBeVisible()
  await again.getByLabel("Amount (EUR)").fill("21.00")
  await again.getByLabel("Paid with").selectOption({ label: "A credit note of this customer" })
  await again.getByLabel("Credit note").selectOption({ label: "2026-0009 · -€21.00 open" })
  await page.getByRole("dialog").getByRole("button", { name: "Record payment" }).click()
  await expect(page.getByRole("dialog")).toHaveCount(0)
  expect(api.callsTo("POST", "/workspace/:ws/invoices/:id/payments")[1]!.body).toMatchObject({ amount: "21.00", settle_invoice_id: "cn-1" })
  expect(seen.at(-1)).toContain("relation_id=rel-1")
  expect(seen.at(-1)).toContain("is_credit=true")
  expect(seen.at(-1)).toContain("status=pending")
  await expectNoRawValues(page, { allow })
})

test.describe(() => {
  test.use({ allow: { responses: [{ url: "/v1/workspace/w1/invoices/inv-1/payments", status: 422 }] } })

  test("pay: the API's refusal shows in the dialog", async ({ page, api }) => {
    invoiceApi(api, invoice({ status: "pending", invoice_nr: "2026-0007", allowed_actions: ["pay"] }))
    api.on("POST", "/workspace/:ws/invoices/:id/payments", () => ({
      status: 422,
      body: { message: "The amount must have at most 2 decimal places.", errors: { amount: ["The amount must have at most 2 decimal places."] } },
    }))
    await page.goto("/invoices/inv-1")
    await page.getByRole("button", { name: "Record payment" }).click()
    // The amount goes as typed; the API judges it.
    await page.getByRole("dialog").getByLabel("Amount (EUR)").fill("12.505")
    await page.getByRole("dialog").getByRole("button", { name: "Record payment" }).click()
    await expect(page.getByRole("dialog").getByText("The amount must have at most 2 decimal places.").first()).toBeVisible()
    expect(api.callsTo("POST", "/workspace/:ws/invoices/:id/payments")[0]!.body).toMatchObject({ amount: "12.505" })
  })
})

test("void a payment after asking", async ({ page, api }) => {
  const payments = [{ id: "pay-1", date: "2026-11-01T00:00:00Z", payment_method: "cash", amount: "100.00000000", amount_minor: 10000 }]
  const state = invoiceApi(
    api,
    invoice({ status: "partially_paid", invoice_nr: "2026-0007", allowed_actions: ["pay"], relationships: { currency: eur, relation, payments, order: null } })
  )
  api.on("DELETE", "/workspace/:ws/invoices/:id/payments/:payment", () => {
    state.inv = { ...state.inv, status: "pending", relationships: { currency: eur, relation, payments: [], order: null } }
    return { message: "ok" }
  })
  await page.goto("/invoices/inv-1")
  await page.getByRole("region", { name: "Payments" }).getByRole("button", { name: "Void" }).click()
  await page.getByRole("alertdialog", { name: "Void this payment?" }).getByRole("button", { name: "Void" }).click()
  await expect(page.getByText("Payment voided.")).toBeVisible()
  await expect(page.getByText("No payments yet.")).toBeVisible()
  expect(api.callsTo("DELETE", "/workspace/:ws/invoices/:id/payments/:payment")[0]!.path).toContain("/payments/pay-1")
})

test("credit everything that's left: lands on the new credit note", async ({ page, api }) => {
  invoiceApi(api, invoice({ status: "paid", invoice_nr: "2026-0007", allowed_actions: ["resend", "duplicate", "credit"] }))
  api.on("POST", "/workspace/:ws/invoices/:id/credit", () => ({ status: 201, body: { data: { invoice_id: "inv-9" } } }))
  await page.goto("/invoices/inv-1")
  await page.getByRole("button", { name: "Credit", exact: true }).click()
  const dialog = page.getByRole("dialog", { name: "Credit lines" })
  await expect(dialog.getByRole("radio", { name: "Everything that's left" })).toBeChecked()
  await dialog.getByRole("button", { name: "Make credit note" }).click()
  await expect(page).toHaveURL(/\/invoices\/inv-9$/)
  expect(api.callsTo("POST", "/workspace/:ws/invoices/:id/credit")[0]!.body).toEqual({ credit_all: true })
})

test.describe(() => {
  test.use({ allow: { responses: [{ url: "/v1/workspace/w1/invoices/inv-1/credit", status: 422 }] } })

  test("credit chosen lines: a quantity over what's left is refused next to the line", async ({ page, api }) => {
    invoiceApi(api, invoice({ status: "paid", invoice_nr: "2026-0007", allowed_actions: ["credit"] }))
    api.on("POST", "/workspace/:ws/invoices/:id/credit", () => ({
      status: 422,
      body: { message: "more items are being credited while the maximum amount is: 10", errors: { "items.0.credit_amount": ["more items are being credited while the maximum amount is: 10"] } },
    }))
    await page.goto("/invoices/inv-1")
    await page.getByRole("button", { name: "Credit", exact: true }).click()
    const dialog = page.getByRole("dialog", { name: "Credit lines" })
    await dialog.getByRole("radio", { name: "Choose lines" }).check()
    await expect(dialog.getByText("10.00 left")).toBeVisible()
    await dialog.getByLabel("Quantity to credit for AFM Honey 500g").fill("11")
    await dialog.getByRole("button", { name: "Make credit note" }).click()
    await expect(dialog.getByText("more items are being credited while the maximum amount is: 10").first()).toBeVisible()
    expect(api.callsTo("POST", "/workspace/:ws/invoices/:id/credit")[0]!.body).toEqual({ items: [{ id: "it-1", credit_amount: 11 }] })
  })
})

test("credit note: mark as credited", async ({ page, api }) => {
  const state = invoiceApi(
    api,
    invoice({ is_credit: true, status: "pending", invoice_nr: "2026-0009", allowed_actions: ["resend", "duplicate", "mark_as_credited"] })
  )
  api.on("POST", "/workspace/:ws/invoices/:id/mark-as-credited", () => {
    state.inv = { ...state.inv, status: "credited", allowed_actions: ["resend", "duplicate"] }
    return { message: "ok" }
  })
  await page.goto("/invoices/inv-1")
  await expect(page.getByRole("heading", { name: "Credit note 2026-0009" })).toBeVisible()
  await page.getByRole("button", { name: "Mark as credited" }).click()
  const dialog = page.getByRole("dialog", { name: "Mark as credited" })
  await dialog.getByLabel("Paid with").selectOption({ label: "Bank transfer or other" })
  await dialog.getByRole("button", { name: "Mark as credited" }).click()
  await expect(page.getByText("Credit note marked as credited.")).toBeVisible()
  await expect(page.getByText("Credited").first()).toBeVisible()
  expect(api.callsTo("POST", "/workspace/:ws/invoices/:id/mark-as-credited")[0]!.body).toMatchObject({ payment_method: "manual" })
})

test("cancel a draft after asking; duplicate opens the copy", async ({ page, api }) => {
  const state = invoiceApi(api, invoice())
  api
    .on("DELETE", "/workspace/:ws/invoices/:id", () => {
      state.inv = { ...state.inv, status: "cancelled", invoice_nr: "N/A", allowed_actions: ["duplicate"] }
      return { message: "ok" }
    })
    .on("POST", "/workspace/:ws/invoices/:id/duplicate", () => ({ status: 201, body: { data: { invoice_id: "inv-2" } } }))
  await page.goto("/invoices/inv-1")
  await page.getByRole("button", { name: "Cancel draft" }).click()
  const ask = page.getByRole("alertdialog", { name: "Cancel this draft?" })
  await ask.getByRole("button", { name: "Cancel draft" }).click()
  await expect(page.getByText("Draft cancelled.")).toBeVisible()
  await expect(page.getByRole("button", { name: "Send", exact: true })).toHaveCount(0)
  await expect(page.getByText("N/A")).toHaveCount(0)
  await page.getByRole("button", { name: "Duplicate" }).click()
  await expect(page).toHaveURL(/\/invoices\/inv-2$/)
})

test("edit details: reference, note and due date", async ({ page, api }) => {
  invoiceApi(api, invoice({ reference: "PO 12" }))
  api.on("POST", "/workspace/:ws/invoices/:id", () => ({ message: "ok" }))
  await page.goto("/invoices/inv-1")
  await page.getByRole("button", { name: "Edit details" }).click()
  const dialog = page.getByRole("dialog", { name: "Edit details" })
  await expect(dialog.getByLabel("Reference")).toHaveValue("PO 12")
  await dialog.getByLabel("Note").fill("Deliver Friday")
  await dialog.getByLabel("Due date").fill("2026-12-01")
  await dialog.getByRole("button", { name: "Save" }).click()
  await expect(page.getByText("Details saved.")).toBeVisible()
  expect(api.callsTo("POST", "/workspace/:ws/invoices/:id")[0]!.body).toEqual({
    relation_id: "rel-1",
    reference: "PO 12",
    note: "Deliver Friday",
    due_date: "2026-12-01",
  })
})

test("an order's invoice links back to its order; nothing left to do says so", async ({ page, api }) => {
  invoiceApi(
    api,
    invoice({ status: "credited", invoice_nr: "2026-0007", allowed_actions: [], relationships: { currency: eur, relation, payments: [], order: { id: "o-1", number: "ORD-0029" } } })
  )
  await page.goto("/invoices/inv-1")
  await expect(page.getByRole("link", { name: "Open order ORD-0029" })).toHaveAttribute("href", "/orders/o-1")
  await expect(page.getByText("Nothing left to do on this invoice.")).toBeVisible()
  await expectNoRawValues(page, { allow })
  await expectFits(page)
})

test("a member without the send right sees no Send", async ({ page, api }) => {
  invoiceApi(api, invoice())
  api.on("GET", "/me", () => ({
    data: { user: { id: "u2", name: "Mo Member", initials: "MM", permissions: [{ workspace: WS, permissions: ["invoice.list", "invoice.view", "invoice.update"] }] } },
  }))
  await page.goto("/invoices/inv-1")
  await expect(page.getByRole("button", { name: "Edit details" })).toBeVisible()
  await expect(page.getByRole("button", { name: "Send", exact: true })).toHaveCount(0)
  await expect(page.getByRole("button", { name: "Duplicate" })).toHaveCount(0)
})
