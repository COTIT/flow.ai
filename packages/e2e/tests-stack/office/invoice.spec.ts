import { expectNoRawValues } from "../../src/test"
import { type Api, apiAs, cred, expect, loginOffice, stackAllow, test, unique, urls, workspaces } from "../../src/stack"

type Order = { id: string; number: string; warehouse_id: string; invoice_id: string | null }
type Invoice = { status: string; invoice_nr: string | null; relationships: { payments: { id: string }[] } }

/** A customer of the test's own, with a contact person and a billing address (a send needs both). */
async function customer(owner: Api, tag: string) {
  const { relation_id } = await owner.post<{ relation_id: string }>(owner.ws("/relations"), {
    type: "business",
    company_name: `e2e invoice customer ${tag}`,
    contact: { first_name: "Ada", last_name: `Miller ${tag}`, email: `ada-${tag}@example.test` },
    addresses: [
      { address: "Mill Lane", house_number: "4", zipcode: "1234AB", city: "Utrecht", country: "NL", type: "billing", main_address: true },
    ],
  })
  return relation_id
}

/** An open order of 3 jars at 5.00 for that customer, from stock of the test's own. */
async function openOrder(owner: Api) {
  const tag = unique()
  const { product_id: product } = await owner.post<{ product_id: string }>(owner.ws("/products"), {
    name: `e2e invoice jar ${tag}`,
    price: "5.00",
    has_stock: true,
  })
  const relation_id = await customer(owner, tag)
  const { order_id: id } = await owner.post<{ order_id: string }>(owner.ws("/orders"), {
    relation_id,
    lines: [{ product_id: product, quantity: "3" }],
  })
  const order = await owner.get<Order>(owner.ws(`/orders/${id}`))
  await owner.post(owner.ws(`/products/${product}/set-stock-levels`), {
    warehouses: [{ warehouse_id: order.warehouse_id, stock_lvl: "10.00", reason: "e2e" }],
  })
  await owner.post(owner.ws(`/orders/${id}/confirm`))
  return order
}

/** A standalone draft invoice of one 10.00 fee line for a customer of the test's own. */
async function draftInvoice(owner: Api) {
  const relation_id = await customer(owner, unique())
  const { invoice_id } = await owner.post<{ invoice_id: string }>(owner.ws("/invoices"), {
    relation_id,
    items: [{ title: "e2e fee", quantity: "1", price: "10", vat: "21" }],
  })
  return invoice_id
}

test("order → invoice → sent → partly paid → paid → credit note → credited, all in office", async ({ page, request }) => {
  const owner = await apiAs(request, cred("office (AFM)", "owner"))
  const order = await openOrder(owner)
  await loginOffice(page, cred("office (AFM)", "owner"))
  await page.goto(`${urls.office}/orders/${order.id}`)
  await page.getByRole("button", { name: "Make invoice" }).click()
  await page.getByRole("link", { name: "Open invoice" }).click()
  await expect(page.getByRole("heading", { name: "Draft invoice" })).toBeVisible()
  const next = page.getByRole("region", { name: "Next step" })
  await expect(next.getByText("A draft gets its number when you send it.")).toBeVisible()
  await expect(page.getByRole("link", { name: `Open order ${order.number}` })).toBeVisible()

  // Sent: numbered, even though the stack has no PDF renderer.
  await next.getByRole("button", { name: "Send", exact: true }).click()
  await page.getByRole("dialog", { name: "Send the invoice" }).getByRole("button", { name: "Send", exact: true }).click()
  await expect(page.getByText("Invoice sent.")).toBeVisible()
  await expect(page.getByRole("heading", { name: /^Invoice \S+$/ })).toBeVisible()
  const invoiceId = page.url().split("/").at(-1)!
  const sent = await owner.get<Invoice>(owner.ws(`/invoices/${invoiceId}`))
  expect(sent.status).toBe("pending")
  expect(sent.invoice_nr).toBeTruthy()
  await expect(page.getByText("Sent", { exact: true }).first()).toBeVisible()

  // Part, then the rest (prefilled with what is open).
  await next.getByRole("button", { name: "Record payment" }).click()
  let dialog = page.getByRole("dialog", { name: "Record a payment" })
  await dialog.getByLabel(/^Amount/).fill("5.00")
  await dialog.getByRole("button", { name: "Record payment" }).click()
  await expect(page.getByText("Payment recorded.")).toBeVisible()
  await expect(page.getByText("Partly paid", { exact: true }).first()).toBeVisible()
  await next.getByRole("button", { name: "Record payment" }).click()
  dialog = page.getByRole("dialog", { name: "Record a payment" })
  await dialog.getByLabel("Paid with").selectOption({ label: "Bank transfer or other" })
  await dialog.getByRole("button", { name: "Record payment" }).click()
  await expect(dialog).toBeHidden()
  // Paid: nothing more to pay, and the API agrees.
  await expect(next.getByRole("button", { name: "Record payment" })).toHaveCount(0)
  expect((await owner.get<Invoice>(owner.ws(`/invoices/${invoiceId}`))).status).toBe("paid")
  await expectNoRawValues(page, { allow: [order.number, sent.invoice_nr!] })

  // The order shows its invoice's status, in words.
  await page.getByRole("link", { name: `Open order ${order.number}` }).click()
  await expect(page.getByRole("region", { name: "Customer" }).getByText("Paid", { exact: true })).toBeVisible()
  await page.getByRole("link", { name: "Open invoice" }).click()

  // Credit everything: a draft credit note, sent, then marked credited.
  await page.getByRole("region", { name: "Next step" }).getByRole("button", { name: "Credit", exact: true }).click()
  await page.getByRole("dialog", { name: "Credit lines" }).getByRole("button", { name: "Make credit note" }).click()
  await expect(page.getByRole("heading", { name: "Draft credit note" })).toBeVisible()
  await next.getByRole("button", { name: "Send", exact: true }).click()
  await page.getByRole("dialog", { name: "Send the invoice" }).getByRole("button", { name: "Send", exact: true }).click()
  await expect(page.getByRole("heading", { name: /^Credit note \S+$/ })).toBeVisible()
  await next.getByRole("button", { name: "Mark as credited" }).click()
  await page.getByRole("dialog", { name: "Mark as credited" }).getByRole("button", { name: "Mark as credited" }).click()
  await expect(page.getByText("Credit note marked as credited.")).toBeVisible()
  await expect(page.getByText("Nothing left to do on this invoice.").or(next.getByRole("button", { name: "Send again" }))).toBeVisible()
  const note = await owner.get<Invoice>(owner.ws(`/invoices/${page.url().split("/").at(-1)}`))
  expect(note.status).toBe("credited")
  await expectNoRawValues(page, { allow: [note.invoice_nr!, sent.invoice_nr!] })
})

test("void a payment: back to sent; cancel a draft: no number, no actions but Duplicate", async ({ page, request }) => {
  const owner = await apiAs(request, cred("office (AFM)", "owner"))
  const paid = await draftInvoice(owner)
  const detail = await owner.get<{ relationships: { relation: { relationships: { contacts: { id: string }[] }; addresses: object[] } } }>(
    owner.ws(`/invoices/${paid}`)
  )
  await owner.post(owner.ws(`/invoices/${paid}/send`), {
    send_method: "manual",
    addressed_contact_id: detail.relationships.relation.relationships.contacts[0]!.id,
    address: { address: "Mill Lane", house_number: "4", zipcode: "1234AB", city: "Utrecht", country: "NL" },
  })
  await owner.post(owner.ws(`/invoices/${paid}/payments`), { date: "2026-11-05", amount: "4.00", payment_method: "cash" })

  await loginOffice(page, cred("office (AFM)", "owner"))
  await page.goto(`${urls.office}/invoices/${paid}`)
  await expect(page.getByText("Partly paid", { exact: true }).first()).toBeVisible()
  await page.getByRole("region", { name: "Payments" }).getByRole("button", { name: "Void" }).click()
  await page.getByRole("alertdialog", { name: "Void this payment?" }).getByRole("button", { name: "Void" }).click()
  await expect(page.getByText("Payment voided.")).toBeVisible()
  await expect(page.getByText("No payments yet.")).toBeVisible()
  expect((await owner.get<Invoice>(owner.ws(`/invoices/${paid}`))).status).toBe("pending")

  const draft = await draftInvoice(owner)
  await page.goto(`${urls.office}/invoices/${draft}`)
  await page.getByRole("button", { name: "Cancel draft" }).click()
  await page.getByRole("alertdialog", { name: "Cancel this draft?" }).getByRole("button", { name: "Cancel draft" }).click()
  await expect(page.getByText("Draft cancelled.")).toBeVisible()
  await expect(page.getByText("Cancelled", { exact: true }).first()).toBeVisible()
  const next = page.getByRole("region", { name: "Next step" })
  await expect(next.getByRole("button")).toHaveText(["Duplicate"])
  await expect(page.getByText("N/A")).toHaveCount(0)
  await expectNoRawValues(page)
})

test.describe(() => {
  // Made before the test, so the allowed answers can name it (allow is read as the test starts).
  let invoice: string
  test.beforeAll(async ({ request }) => {
    invoice = await draftInvoice(await apiAs(request, cred("office (AFM)", "owner")))
  })
  test.use({
    // eslint-disable-next-line no-empty-pattern -- Playwright reads fixture deps from the pattern
    allow: async ({}, use) =>
      use({
        ...stackAllow,
        responses: [
          ...stackAllow.responses,
          // The stack has no PDF renderer: an email send and Make PDF answer 503.
          { url: `${urls.office}/v1/workspace/${workspaces.AFM}/invoices/${invoice}/send`, status: 503 },
          { url: `${urls.office}/v1/workspace/${workspaces.AFM}/invoices/${invoice}/preview`, status: 503 },
          // Another workspace's admin asks for it through their own workspace.
          { url: `${urls.office}/v1/workspace/${workspaces.RUAHA}/invoices/${invoice}`, status: 404 },
        ],
      }),
  })

  test("without a renderer: email and PDF say so in words, and the draft stays a draft", async ({ page, request }) => {
    const owner = await apiAs(request, cred("office (AFM)", "owner"))
    await loginOffice(page, cred("office (AFM)", "owner"))
    await page.goto(`${urls.office}/invoices/${invoice}`)
    await page.getByRole("button", { name: "Send", exact: true }).click()
    const dialog = page.getByRole("dialog", { name: "Send the invoice" })
    await dialog.getByRole("radio", { name: "Email to the customer" }).check()
    await dialog.getByRole("button", { name: "Send", exact: true }).click()
    await expect(dialog.getByText("Email sending isn't set up yet.")).toBeVisible()
    await dialog.getByRole("button", { name: "Cancel" }).click()
    expect((await owner.get<Invoice>(owner.ws(`/invoices/${invoice}`))).status).toBe("draft")
    await page.getByRole("button", { name: "Make PDF" }).click()
    await expect(page.getByText("PDFs aren't available on this server yet.")).toBeVisible()
    await expectNoRawValues(page)
  })

  test("another workspace's admin can't open it", async ({ page }) => {
    await loginOffice(page, cred("office (RUAHA)", "admin"))
    await page.goto(`${urls.office}/invoices/${invoice}`)
    await expect(page.getByRole("button", { name: "Send", exact: true })).toHaveCount(0)
    await expect(page.getByText("e2e fee")).toHaveCount(0)
  })
})
