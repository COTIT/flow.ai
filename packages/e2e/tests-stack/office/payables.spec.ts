import type { APIRequestContext, Page } from "@playwright/test"

import {
  activeBook,
  addAccount,
  command,
  freshWorkspace,
} from "../../src/accounting"
import { expectNoRawValues, pick } from "../../src/test"
import {
  type Api,
  apiAs,
  expect,
  loginOffice,
  stackAllow,
  test,
  unique,
  urls,
} from "../../src/stack"

// Flow ERP M7-A: supplier bills, supplier payments and purchase returns on the Office screens,
// in a fresh workspace with an active book (payables refuse setup books).

type Po = { id: string; version: number; lines: { id: string }[] }
type Receipt = { id: string; version: number; lines: { id: string }[] }
const today = () => new Date().toISOString().slice(0, 10)
const allow = [/\bPO-\d+\b/, /\bGR-\d+\b/]

/** Confirmed order of `qty` × product at 4.00, received and booked (active book: valued). */
type Base = {
  supplier: string
  currency: string
  warehouse: string
  product: string
}
async function bookedReceipt(
  request: APIRequestContext,
  api: Api,
  w: Base,
  qty: string,
  landed: { amount: string; evidence_ref: string }[] = []
): Promise<{ po: string; receipt: string }> {
  let po: Po = await command<Po>(request, api, "POST", "/purchase-orders", {
    payload: {
      supplier_id: w.supplier,
      currency_id: w.currency,
      purchase_date: today(),
      default_warehouse_id: w.warehouse,
      lines: [{ product_id: w.product, quantity: qty, unit_price: "4.00" }],
    },
  })
  const review = await api.post<{ review_fingerprint: string }>(
    api.ws(`/purchase-orders/${po.id}/confirm/review`),
    {}
  )
  po = await command<Po>(
    request,
    api,
    "POST",
    `/purchase-orders/${po.id}/confirm`,
    {
      expected_version: po.version,
      payload: { review_fingerprint: review.review_fingerprint },
    }
  )
  const receipt = await command<Receipt>(
    request,
    api,
    "POST",
    `/purchase-orders/${po.id}/receipts`,
    {
      payload: {
        effective_receipt_date: today(),
        lines: [
          {
            po_line_id: po.lines[0]!.id,
            base_quantity: qty,
            warehouse_id: w.warehouse,
          },
        ],
      },
    }
  )
  const book = {
    receipt_line_ids: receipt.lines.map((l) => l.id),
    landed_costs: landed.map((l) => ({
      client_component_ref: crypto.randomUUID(),
      currency_id: w.currency,
      ...l,
    })),
  }
  const path = `/purchase-orders/${po.id}/receipts/${receipt.id}/book`
  const r = await api.post<{ review_fingerprint: string }>(
    api.ws(`${path}/review`),
    book
  )
  await command(request, api, "POST", path, {
    expected_version: receipt.version,
    payload: { ...book, review_fingerprint: r.review_fingerprint },
  })
  return { po: po.id, receipt: receipt.id }
}

type World = Awaited<ReturnType<typeof makeWorld>>

/** Active book, a store, a supplier, a stock product, an expense account and a bank account. */
async function makeWorld(request: APIRequestContext) {
  const tag = unique()
  const { login, api } = await freshWorkspace(request, "payables")
  const { currency } = await activeBook(request, api)
  await addAccount(request, api, "6100", "Office rent", "expense")
  const bankGl = await addAccount(request, api, "1120", "Main bank", "asset")
  await command(request, api, "POST", "/accounting/bank-accounts", {
    payload: {
      name: "Main bank",
      currency_id: currency.id,
      account_id: bankGl.id,
    },
  })
  await api.post(api.ws("/warehouses"), {
    name: "Shop floor",
    is_store: true,
    default_warehouse: true,
  })
  const warehouse = (
    await api.get<{ id: string; name: string }[]>(
      api.ws("/warehouses?page_size=100")
    )
  ).find((x) => x.name === "Shop floor")!.id
  const types = await api.get<{ id: string }[]>(
    api.ws("/custom-types?domain=relation&is_supplier=1")
  )
  const { relation_id: supplier } = await api.post<{ relation_id: string }>(
    api.ws("/relations"),
    {
      type: "business",
      company_name: `Bees ${tag}`,
      relation_type_id: types[0]!.id,
    }
  )
  const { product_id: product } = await api.post<{ product_id: string }>(
    api.ws("/products"),
    {
      name: `Wax ${tag}`,
      price: "5.00",
      has_stock: true,
    }
  )
  const w = {
    tag,
    login,
    api,
    currency: currency.id,
    code: currency.asset_type.toUpperCase(),
    supplier,
    product,
    warehouse,
  }
  const first = await bookedReceipt(request, api, w, "5")
  const second = await bookedReceipt(request, api, w, "5")
  // A draft bill for the first receipt, so its refused match has a known URL.
  const bill = await command<{ id: string; number: number }>(
    request,
    api,
    "POST",
    "/supplier-bills",
    {
      payload: {
        supplier_id: supplier,
        currency_id: currency.id,
        supplier_invoice_number: `INV-${tag}`,
        invoice_date: today(),
        accounting_date: today(),
        lines: [{ product_id: product, quantity: "5", unit_price: "4.00" }],
      },
    }
  )
  return { ...w, first, second, bill }
}

const side = (page: Page, name: string) => page.getByRole("region", { name })

test.describe.configure({ mode: "serial" })

test.describe("payables", () => {
  let w: World
  test.beforeAll(async ({ request }) => {
    w = await makeWorld(request)
  })
  test.use({
    // eslint-disable-next-line no-empty-pattern -- Playwright reads fixture deps from the pattern
    allow: async ({}, use) => {
      const ws = `${urls.office}/v1/workspace/${w.api.workspace}`
      use({
        ...stackAllow,
        responses: [
          ...stackAllow.responses,
          { url: `${ws}/supplier-bills`, status: 422 },
          { url: `${ws}/supplier-bills/${w.bill.id}/match`, status: 422 },
          { url: `${ws}/purchase-returns`, status: 422 },
        ],
      })
    },
  })

  test("supplier bill: duplicate invoice refused, expense bill drafted; empty lists say so", async ({
    page,
  }) => {
    await loginOffice(page, w.login)
    await page.goto(`${urls.office}/supplier-payments`)
    await expect(page.getByText("No supplier payments yet.")).toBeVisible()
    await page.goto(`${urls.office}/supplier-bills`)
    await expect(
      page.getByRole("link", { name: `Bill ${w.bill.number}` })
    ).toBeVisible()
    await page.getByRole("link", { name: "New bill" }).click()
    await expect(
      page.getByRole("heading", { name: "New supplier bill" })
    ).toBeVisible()
    await page
      .getByLabel("Supplier", { exact: true })
      .selectOption({ label: `Bees ${w.tag}` })
    await page.getByLabel("Supplier invoice number").fill(`INV-${w.tag}`)
    await page.getByLabel("Line type").selectOption("expense")
    await pick(page.getByLabel("Expense account"), "6100 Office rent")
    await page.getByLabel("Quantity").fill("1")
    await page.getByLabel("Unit price").fill("120")
    await page.getByRole("button", { name: "Save draft" }).click()
    // Same supplier, currency and invoice number: refused in words, nothing saved.
    await expect(page.locator("#bill-invoice-error")).toContainText(
      "already on a bill for this supplier"
    )
    await page.getByLabel("Supplier invoice number").fill(`RENT-${w.tag}`)
    await page.getByRole("button", { name: "Save draft" }).click()
    await expect(page.getByText("Draft saved.")).toBeVisible()
    await expect(
      page.getByRole("heading", { name: /^Bill \d+$/ })
    ).toBeVisible()
    await expect(
      side(page, "Next step").getByRole("button", { name: "Post bill" })
    ).toBeVisible()
    await expectNoRawValues(page, { allow })
  })

  test("supplier bill: match over capacity refused, match, post; pay by bank and apply", async ({
    page,
  }) => {
    await loginOffice(page, w.login)
    await page.goto(`${urls.office}/supplier-bills/${w.bill.id}`)
    await side(page, "Next step")
      .getByRole("button", { name: "Match received goods" })
      .click()
    let dialog = page.getByRole("dialog")
    const qty = dialog.getByLabel(/Receipt \d+ \(order \d+\), 5 left/).first()
    await qty.fill("6")
    await dialog.getByRole("button", { name: "Save matches" }).click()
    await expect(
      dialog.getByText(/More than this receipt line still has to bill/).first()
    ).toBeVisible()
    await qty.fill("5")
    await dialog.getByRole("button", { name: "Save matches" }).click()
    await expect(page.getByText("Received goods matched.")).toBeVisible()

    await side(page, "Next step")
      .getByRole("button", { name: "Post bill" })
      .click()
    dialog = page.getByRole("dialog")
    await expect(dialog.getByText("Check before you confirm")).toBeVisible()
    await expect(dialog).toContainText(`${w.code} 20.00`)
    await dialog.getByRole("button", { name: "Post bill" }).click()
    await expect(page.getByText("Posted.")).toBeVisible()
    await expect(
      page.getByText("Posted", { exact: true }).first()
    ).toBeVisible()
    await expectNoRawValues(page, { allow })

    // Pay 20.00 from the bank, then apply it to the bill the server offers.
    await page.goto(`${urls.office}/supplier-payments/new`)
    await expect(
      page.getByRole("heading", { name: "Record a supplier payment or refund" })
    ).toBeVisible()
    await page
      .getByLabel("Supplier", { exact: true })
      .selectOption({ label: `Bees ${w.tag}` })
    await page.getByLabel("Amount").fill("20.00")
    await page
      .getByLabel("Paid from / into")
      .selectOption({ label: "Main bank" })
    await page.getByLabel("Bank reference").fill(`TX-${w.tag}`)
    await page.getByLabel("Document reference").fill("Statement 1")
    await page.getByRole("button", { name: "Review" }).click()
    await expect(page.getByText("Check before you confirm")).toBeVisible()
    await page.getByRole("button", { name: "Record" }).click()
    await expect(page.getByText("Recorded.")).toBeVisible()
    await side(page, "Next step")
      .getByRole("button", { name: "Apply to a bill" })
      .click()
    dialog = page.getByRole("dialog")
    await dialog.getByRole("radio").first().check()
    await expect(dialog.getByLabel("Amount")).toHaveValue("20.00")
    await dialog.getByRole("button", { name: "Review" }).click()
    await dialog.getByRole("button", { name: "Apply to a bill" }).click()
    await expect(page.getByText("Applied.")).toBeVisible()
    await expect(page.getByText("Not yet applied").locator("..")).toContainText(
      `${w.code} 0.00`
    )
    await expectNoRawValues(page, { allow })
  })

  test("purchase return: from the receipt, over capacity refused, post, reverse", async ({
    page,
    request,
  }) => {
    await loginOffice(page, w.login)
    await page.goto(
      `${urls.office}/purchases/${w.second.po}/receipts/${w.second.receipt}`
    )
    await page.getByRole("link", { name: "Return goods" }).click()
    await page
      .getByLabel("Return from")
      .selectOption({ label: "Not billed yet (5 left)" })
    await page.getByLabel("Quantity").fill("6")
    await page.getByLabel("Reason").fill("Damaged jars")
    await page.getByRole("button", { name: "Save draft" }).click()
    await expect(page.getByText(/More than/).first()).toBeVisible()
    await page.getByLabel("Quantity").fill("2")
    await page.getByRole("button", { name: "Save draft" }).click()
    await expect(page.getByText("Draft saved.")).toBeVisible()
    await expect(
      page.getByRole("heading", { name: /^Return \d+$/ })
    ).toBeVisible()

    await side(page, "Next step")
      .getByRole("button", { name: "Post return" })
      .click()
    let dialog = page.getByRole("dialog")
    await expect(dialog.getByText("Check before you confirm")).toBeVisible()
    await dialog.getByRole("button", { name: "Post return" }).click()
    await expect(page.getByText("Posted.")).toBeVisible()

    await side(page, "Next step")
      .getByRole("button", { name: "Reverse" })
      .click()
    dialog = page.getByRole("dialog")
    await dialog.getByLabel("Reason").fill("Supplier refused the return")
    await dialog.getByRole("button", { name: "Review" }).click()
    await dialog.getByRole("button", { name: "Reverse" }).click()
    await expect(page.getByText("Reversed.")).toBeVisible()
    await expectNoRawValues(page, { allow })

    // Another account's owner can't read this workspace's return.
    const other = await apiAs(
      request,
      (await freshWorkspace(request, "payables-other")).login
    )
    const id = page.url().split("?")[0]!.split("/").at(-1)!
    const res = await request.get(
      `${urls.api}/v1/workspace/${w.api.workspace}/purchase-returns/${id}`,
      {
        headers: { Authorization: `Bearer ${other.token}` },
      }
    )
    expect([403, 404]).toContain(res.status())
  })

  test("supplier bill: extra cost picked from the booked receipt, price prefilled, posted", async ({
    page,
    request,
  }) => {
    await loginOffice(page, w.login)
    await page.goto(`${urls.office}/supplier-bills/new`)
    await page.getByLabel("Line type").selectOption("landed_cost")
    // Nothing booked yet: the form says so instead of an empty picker.
    await expect(
      page.getByText("No booked extra cost is left to bill in this currency.")
    ).toBeVisible()

    const freight = `DHL-${w.tag}`
    const booked = await bookedReceipt(
      request,
      await apiAs(request, w.login),
      w,
      "2",
      [{ amount: "7.00", evidence_ref: freight }]
    )
    await page.reload()
    await page
      .getByLabel("Supplier", { exact: true })
      .selectOption({ label: `Bees ${w.tag}` })
    await page.getByLabel("Supplier invoice number").fill(`FRT-${w.tag}`)
    await page.getByLabel("Line type").selectOption("landed_cost")
    const option = page
      .getByLabel("Extra cost", { exact: true })
      .locator("option", { hasText: freight })
    await page
      .getByLabel("Extra cost", { exact: true })
      .selectOption({ label: (await option.textContent())!.trim() })
    await expect(page.getByLabel("Quantity")).toHaveValue("1")
    await expect(page.getByLabel("Unit price")).toHaveValue("7.00")
    await page.getByLabel("Unit price").fill("7.50")
    await page.getByRole("button", { name: "Save draft" }).click()
    await expect(page.getByText("Draft saved.")).toBeVisible()
    await expect(
      page.getByText(new RegExp(`${freight} · booked`))
    ).toBeVisible()

    // The component is now held by this draft: a new bill no longer offers it.
    const billUrl = page.url().split("?")[0]!
    await page.goto(`${urls.office}/supplier-bills/new`)
    await page.getByLabel("Line type").selectOption("landed_cost")
    await expect(
      page.getByText("No booked extra cost is left to bill in this currency.")
    ).toBeVisible()

    // Its own edit form still offers it, selected.
    await page.goto(`${billUrl}/edit`)
    await expect(page.getByLabel("Extra cost", { exact: true })).toContainText(
      freight
    )

    await page.goto(billUrl)
    await side(page, "Next step")
      .getByRole("button", { name: "Post bill" })
      .click()
    const dialog = page.getByRole("dialog")
    await expect(dialog.getByText("Check before you confirm")).toBeVisible()
    await dialog.getByRole("button", { name: "Post bill" }).click()
    await expect(page.getByText("Posted.")).toBeVisible()
    await expect(
      page.getByRole("link", { name: "receipt", exact: true })
    ).toHaveAttribute(
      "href",
      `/purchases/${booked.po}/receipts/${booked.receipt}`
    )
    await expectNoRawValues(page, { allow })
  })
})
