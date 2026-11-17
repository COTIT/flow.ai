import { expectNoRawValues } from "../../src/test"
import { expect, expectFits, me, openMenu, paged, test, type FakeApi } from "./fake-api"

const EUR = { id: "cur-eur", code: "EUR", decimals: 2 }
const allow = [/\bPO-\d+\b/, /\bGR-\d+\b/]

type Line = {
  id: string
  line_no: number
  product: { id: string; name: string; unit: string | null }
  purchase_unit: null
  input_quantity: string
  ratio: string
  base_quantity: string
  unit_price: string
  extension: string
  warehouse_id: string | null
  received_quantity: string
  booked_quantity: string
  closed_quantity: string
  open_quantity: string
}

const line = (over: Partial<Line> = {}): Line => ({
  id: "pol-1",
  line_no: 1,
  product: { id: "p1", name: "Ilundo Wild Forest Honey", unit: "jar" },
  purchase_unit: null,
  input_quantity: "5.00",
  ratio: "1",
  base_quantity: "5.00",
  unit_price: "4.00",
  extension: "20.00",
  warehouse_id: null,
  received_quantity: "0.00",
  booked_quantity: "0.00",
  closed_quantity: "0.00",
  open_quantity: "5.00",
  ...over,
})

function order(over: Record<string, unknown> = {}) {
  return {
    id: "po-1",
    workspace_id: "w1",
    number: 1001,
    version: 1,
    state: "draft",
    receipt_status: "not_received",
    supplier: { id: "sup-1", name: "Mbeya Jars Ltd" },
    supplier_snapshot: null,
    currency: EUR,
    purchase_date: "2026-11-10",
    expected_date: null,
    reference: null,
    note: null,
    default_warehouse_id: "wh-1",
    totals: { ordered: "20.00", received: "0.00", open: "20.00" },
    lines: [line()],
    created_by: "u1",
    created_at: "2026-11-10T08:00:00Z",
    updated_at: "2026-11-10T08:00:00Z",
    confirmed_by: null,
    confirmed_at: null,
    cancelled_at: null,
    cancel_reason: null,
    allowed_actions: ["update", "confirm", "cancel"],
    business_date: "2026-11-15",
    ...over,
  }
}

const confirmed = () =>
  order({
    version: 2,
    state: "confirmed",
    receipt_status: "partial",
    supplier_snapshot: { company_name: "Mbeya Jars Ltd" },
    totals: { ordered: "20.00", received: "8.00", open: "12.00" },
    lines: [line({ received_quantity: "2.00", open_quantity: "3.00" })],
    allowed_actions: ["receive", "close"],
  })

const receipt = (over: Record<string, unknown> = {}) => ({
  id: "rc-1",
  purchase_order_id: "po-1",
  number: 7,
  version: 1,
  effective_receipt_date: "2026-11-12",
  note: null,
  recorded_at: "2026-11-12T09:00:00Z",
  created_by: "u1",
  booking_status: "unbooked",
  lines: [
    {
      id: "rl-1",
      po_line_id: "pol-1",
      product_id: "p1",
      warehouse_id: "wh-1",
      bin_id: null,
      lot_id: null,
      base_quantity: "2.00",
      state: "pending",
      booked_at: null,
      accounting_date: null,
      net_value: null,
      landed_value: null,
      value: null,
      rate: null,
      journal_entry_id: null,
    },
  ],
  landed_costs: [],
  allowed_actions: ["book"],
  ...over,
})

function purchasing(api: FakeApi, po: ReturnType<typeof order>, receipts: ReturnType<typeof receipt>[] = []) {
  const state = { po, receipts }
  api
    .on("GET", "/workspace/:ws/purchase-orders", ({ query }) =>
      paged([state.po].filter((p) => !query.get("state") || p.state === query.get("state")))
    )
    .on("GET", "/workspace/:ws/purchase-orders/:id", () => ({ data: state.po }))
    .on("GET", "/workspace/:ws/purchase-orders/:id/receipts", () => paged(state.receipts))
    .on("GET", "/workspace/:ws/purchase-orders/:id/receipts/:r", () => ({ data: state.receipts[0] }))
    .on("GET", "/workspace/:ws/purchase-orders/:id/events", () => ({
      data: [{ at: "2026-11-10T08:00:00Z", kind: "created", actor: "u1", ref_id: "po-1", detail: null }],
    }))
    .on("GET", "/workspace/:ws/warehouses", () => paged([{ id: "wh-1", name: "Dar es Salaam" }]))
    .on("GET", "/workspace/:ws/currencies", () =>
      paged([{ id: "cur-eur", asset_type: "EUR", decimals: 2, default_currency: true }])
    )
    .on("GET", "/workspace/:ws/relations", () =>
      paged([{ id: "sup-1", company_name: "Mbeya Jars Ltd", alias: null, relation_nr: null }])
    )
    .on("GET", "/workspace/:ws/products", () =>
      paged([{ id: "p1", name: "Ilundo Wild Forest Honey" }])
    )
    .on("GET", "/workspace/:ws/products/:id/purchase-units", () => ({ data: [] }))
  return state
}

test("lists purchase orders with the server's totals and filters by state", async ({ page, api }) => {
  purchasing(api, order())
  await page.goto("/purchases")
  const row = page.getByRole("row", { name: /PO-1001/ })
  await expect(row).toContainText("Mbeya Jars Ltd")
  await expect(row).toContainText("Draft")
  await expect(row).toContainText("20.00")
  await expect(page.getByRole("link", { name: "New purchase order" })).toBeVisible()
  await expectFits(page)
  await expectNoRawValues(page, { allow })

  await page.getByRole("combobox", { name: "State" }).click()
  await page.getByRole("option", { name: "Confirmed" }).click()
  await expect(page).toHaveURL(/state=confirmed/)
  await expect(page.getByText("No matches")).toBeVisible()
})

test("an empty list says so", async ({ page, api }) => {
  api.on("GET", "/workspace/:ws/purchase-orders", () => paged([]))
  await page.goto("/purchases")
  await expect(page.getByText("No purchase orders yet")).toBeVisible()
  await expectFits(page)
})

test("the menu shows Purchases under Buy", async ({ page, api }) => {
  purchasing(api, order())
  await page.goto("/purchases")
  await openMenu(page)
  const nav = page.getByRole("navigation", { name: "Main menu" })
  await expect(nav.getByText("Buy")).toBeVisible()
  await expect(nav.getByRole("link", { name: "Purchases" })).toHaveAttribute("aria-current", "page")
})

test.describe("a member without purchase rights", () => {
  test("sees no menu item and no page", async ({ page, api }) => {
    api.on("GET", "/me", () => ({
      data: { user: { ...me, permissions: [{ workspace: "w1", permissions: ["product.list"] }] } },
    }))
    api.on("GET", "/workspace/:ws/products", () => paged([]))
    await page.goto("/purchases")
    await expect(page.getByText(/don.t have access|no access/i).first()).toBeVisible()
    await openMenu(page)
    await expect(
      page.getByRole("navigation", { name: "Main menu" }).getByRole("link", { name: "Purchases" })
    ).toHaveCount(0)
  })
})

test.describe("creating a draft", () => {
  test.use({ allow: { responses: [{ url: "/v1/workspace/w1/purchase-orders", status: 422 }] } })

  test("shows the API's field error next to the line, then saves", async ({ page, api }) => {
    const state = purchasing(api, order())
    let tries = 0
    api.on("POST", "/workspace/:ws/purchase-orders", ({ body }) => {
      tries += 1
      if (tries === 1)
        return {
          status: 422,
          body: {
            message: "The given data was invalid.",
            errors: { "payload.lines.0.quantity": ["Use at most 2 decimals."] },
          },
        }
      state.po = order({ lines: [line({ input_quantity: (body as { payload: { lines: { quantity: string }[] } }).payload.lines[0]!.quantity })] })
      return { status: 201, body: { data: state.po } }
    })
    await page.goto("/purchases/new")
    await page.getByLabel("Supplier", { exact: true }).selectOption("sup-1")
    await expect(page.getByLabel("Currency")).toHaveValue("cur-eur")
    await page.getByLabel("Product").selectOption("p1")
    await page.getByLabel("Quantity", { exact: true }).fill("5.123")
    await page.getByLabel("Unit price").fill("4")
    await expectFits(page)
    await page.getByRole("button", { name: "Save draft" }).click()
    await expect(page.getByText("Use at most 2 decimals.")).toBeVisible()

    await page.getByLabel("Quantity", { exact: true }).fill("5")
    await page.getByRole("button", { name: "Save draft" }).click()
    await expect(page).toHaveURL(/\/purchases\/po-1/)
    await expect(page.getByText("Purchase order saved as draft.")).toBeVisible()
    const sent = api.callsTo("POST", "/workspace/:ws/purchase-orders")
    const last = sent.at(-1)!
    expect((last.body as { client_ref: string }).client_ref).toBe(last.headers["idempotency-key"])
    expect((last.body as { payload: { lines: unknown[] } }).payload.lines).toEqual([
      { product_id: "p1", quantity: "5", unit_price: "4", purchase_unit_id: null, warehouse_id: null },
    ])
  })
})

test("a draft confirms through the server's review and echoes its fingerprint", async ({ page, api }) => {
  const state = purchasing(api, order())
  api.on("POST", "/workspace/:ws/purchase-orders/:id/confirm/review", () => ({
    data: {
      purchase_order_id: "po-1",
      version: 1,
      currency: EUR,
      supplier_snapshot: { company_name: "Mbeya Jars Ltd" },
      lines: [{ line_id: "pol-1", product_id: "p1", base_quantity: "5.00", unit_price: "4.00", extension: "20.00", ratio: "1" }],
      totals: { ordered: "20.00", received: "0.00", open: "20.00" },
      review_fingerprint: "fp-123",
    },
  }))
  api.on("POST", "/workspace/:ws/purchase-orders/:id/confirm", () => {
    state.po = confirmed()
    return { data: state.po }
  })
  await page.goto("/purchases/po-1")
  const next = page.getByRole("region", { name: "Next step" })
  await expect(next.getByRole("link", { name: "Edit draft" })).toBeVisible()
  await expect(next.getByRole("button", { name: "Cancel order" })).toBeVisible()
  await expectFits(page)
  await expectNoRawValues(page, { allow })

  await next.getByRole("button", { name: "Confirm order" }).click()
  const dialog = page.getByRole("dialog")
  await expect(dialog).toContainText("EUR 20.00")
  await dialog.getByRole("button", { name: "Confirm order" }).click()
  await expect(page.getByText("Order confirmed.")).toBeVisible()
  const body = api.callsTo("POST", "/workspace/:ws/purchase-orders/:id/confirm").at(-1)!.body as {
    expected_version: number
    payload: { review_fingerprint: string }
  }
  expect(body.expected_version).toBe(1)
  expect(body.payload.review_fingerprint).toBe("fp-123")
})

test.describe("receiving", () => {
  test.use({ allow: { responses: [{ url: "/v1/workspace/w1/purchase-orders/po-1/receipts", status: 422 }] } })

  test("refuses more than is open on the line, then records the receipt", async ({ page, api }) => {
    const state = purchasing(api, confirmed())
    let tries = 0
    api.on("POST", "/workspace/:ws/purchase-orders/:id/receipts", () => {
      tries += 1
      if (tries === 1)
        return {
          status: 422,
          body: {
            message: "The given data was invalid.",
            errors: { "payload.lines.0.base_quantity": ["Only 3.00 is still open."] },
          },
        }
      state.receipts = [receipt({ number: 8 })]
      return { status: 201, body: { data: state.receipts[0] } }
    })
    await page.goto("/purchases/po-1")
    await page.getByRole("region", { name: "Next step" }).getByRole("button", { name: "Receive goods" }).click()
    const dialog = page.getByRole("dialog")
    await expect(dialog.getByLabel("Warehouse")).toHaveValue("wh-1")
    await dialog.getByLabel(/Ilundo Wild Forest Honey/).fill("4")
    await expectFits(page)
    await dialog.getByRole("button", { name: "Receive goods" }).click()
    await expect(dialog.getByText("Only 3.00 is still open.")).toBeVisible()

    await dialog.getByLabel(/Ilundo Wild Forest Honey/).fill("3")
    await dialog.getByRole("button", { name: "Receive goods" }).click()
    await expect(page.getByText("Receipt GR-8 recorded.")).toBeVisible()
    await expect(page.getByRole("link", { name: "Receipt GR-8" })).toBeVisible()
    const sent = api.callsTo("POST", "/workspace/:ws/purchase-orders/:id/receipts").at(-1)!.body as {
      payload: { lines: unknown[] }
    }
    expect(sent.payload.lines).toEqual([{ po_line_id: "pol-1", base_quantity: "3", warehouse_id: "wh-1" }])
  })
})

test.describe("a changed order", () => {
  test.use({ allow: { responses: [{ url: "/v1/workspace/w1/purchase-orders/po-1/close-lines", status: 409 }] } })

  test("reloads it and says so when the API answers stale_document", async ({ page, api }) => {
    purchasing(api, confirmed())
    api.on("POST", "/workspace/:ws/purchase-orders/:id/close-lines", () => ({
      status: 409,
      body: { message: "The document changed.", code: "stale_document" },
    }))
    await page.goto("/purchases/po-1")
    const reads = api.callsTo("GET", "/workspace/:ws/purchase-orders/:id").length
    await page.getByRole("region", { name: "Next step" }).getByRole("button", { name: "Close remainder" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Reason").fill("Supplier is out of stock")
    await dialog.getByRole("button", { name: "Close remainder" }).click()
    await expect(page.getByRole("main").getByText(/This order changed since you opened it/)).toBeVisible()
    await expect(dialog).toBeHidden()
    await expect.poll(() => api.callsTo("GET", "/workspace/:ws/purchase-orders/:id").length).toBeGreaterThan(reads)
    await expectNoRawValues(page, { allow })
  })
})

test.describe("booking a receipt", () => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/purchase-orders/po-1/receipts/rc-1/book/review", status: 422 },
        { url: "/v1/workspace/w1/purchase-orders/po-1/receipts/rc-1/book", status: 422 },
      ],
    },
  })

  test("reviews first, keeps a cost's ref across retries, and books stock only in a setup book", async ({ page, api }) => {
    const state = purchasing(api, confirmed(), [receipt()])
    api.on("POST", "/workspace/:ws/purchase-orders/:id/receipts/:r/book/review", ({ body }) => {
      const b = body as { landed_costs: unknown[] }
      if (b.landed_costs.length)
        return {
          status: 422,
          body: { message: "unsupported_active_erp_operation", code: "unsupported_active_erp_operation" },
        }
      return {
        data: {
          receipt_id: "rc-1",
          receipt_version: 1,
          purchase_order_version: 2,
          accounting_date: "2026-11-12",
          functional_currency: null,
          journal: null,
          landed_costs: [],
          lines: [{ receipt_line_id: "rl-1", base_quantity: "2.00", net_value: null, landed_value: null, value: null, rate: null }],
          pool: null,
          total: null,
          review_fingerprint: "fp-book",
        },
      }
    })
    api.on("POST", "/workspace/:ws/purchase-orders/:id/receipts/:r/book", () => {
      state.receipts = [
        receipt({ booking_status: "booked", allowed_actions: [], lines: [{ ...receipt().lines[0], state: "booked" }] }),
      ]
      return { data: state.receipts[0] }
    })
    await page.goto("/purchases/po-1/receipts/rc-1")
    await expect(page.getByRole("heading", { name: "Receipt GR-7" })).toBeVisible()
    await expect(page.getByText("Waiting to book")).toBeVisible()
    await expectFits(page)
    await expectNoRawValues(page, { allow })

    await page.getByRole("button", { name: "Book into stock" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByRole("button", { name: "Add cost" }).click()
    await dialog.getByLabel("Amount").fill("5")
    await dialog.getByRole("button", { name: "Review" }).click()
    await expect(dialog.getByText("Extra costs need active accounting.")).toBeVisible()
    await dialog.getByRole("button", { name: "Review" }).click()
    const reviews = "/workspace/:ws/purchase-orders/:id/receipts/:r/book/review"
    await expect.poll(() => api.callsTo("POST", reviews).length).toBe(2)
    await expect(dialog.getByText("Extra costs need active accounting.")).toBeVisible()
    const refs = api
      .callsTo("POST", "/workspace/:ws/purchase-orders/:id/receipts/:r/book/review")
      .map((c) => (c.body as { landed_costs: { client_component_ref: string }[] }).landed_costs[0]!.client_component_ref)
    expect(refs).toHaveLength(2)
    expect(refs[0]).toBe(refs[1])
    await expectNoRawValues(page, { allow })

    await dialog.getByRole("button", { name: /Remove cost 1/ }).click()
    await dialog.getByRole("button", { name: "Review" }).click()
    await expect(dialog.getByText(/Stock only/)).toBeVisible()
    await dialog.getByRole("button", { name: "Book into stock" }).click()
    await expect(page.getByText("Booked into stock.")).toBeVisible()
    const book = api.callsTo("POST", "/workspace/:ws/purchase-orders/:id/receipts/:r/book").at(-1)!.body as {
      expected_version: number
      payload: { review_fingerprint: string; receipt_line_ids: string[] }
    }
    expect(book.payload).toMatchObject({ review_fingerprint: "fp-book", receipt_line_ids: ["rl-1"] })
    expect(book.expected_version).toBe(1)
  })

  test("says to book the earlier receipt first, in words", async ({ page, api }) => {
    purchasing(api, confirmed(), [receipt()])
    api.on("POST", "/workspace/:ws/purchase-orders/:id/receipts/:r/book/review", () => ({
      status: 422,
      body: { message: "receipt_booking_required", code: "receipt_booking_required" },
    }))
    await page.goto("/purchases/po-1/receipts/rc-1")
    await page.getByRole("button", { name: "Book into stock" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByRole("button", { name: "Review" }).click()
    await expect(dialog.getByText("Book the earlier receipt for this product and warehouse first.")).toBeVisible()
    await expectNoRawValues(page, { allow })
  })

  test("a late line can't be chosen and is explained", async ({ page, api }) => {
    purchasing(api, confirmed(), [
      receipt({ allowed_actions: [], lines: [{ ...receipt().lines[0], state: "late" }] }),
    ])
    await page.goto("/purchases/po-1/receipts/rc-1")
    await expect(page.getByText(/Booking late lines comes in a later release/)).toBeVisible()
    await expect(page.getByRole("button", { name: "Book into stock" })).toHaveCount(0)
    await expectFits(page)
  })
})

test("a booked receipt shows its values in the book's currency, as the API sent them", async ({ page, api }) => {
  const TZS = { id: "cur-tzs", code: "TZS", decimals: 2 }
  const booked = receipt({
    booking_status: "booked",
    allowed_actions: [],
    functional_currency: TZS,
    lines: [
      {
        ...receipt().lines[0],
        state: "booked",
        booked_at: "2026-11-12T10:00:00Z",
        accounting_date: "2026-11-12",
        net_value: "21000.00",
        landed_value: "1500.00",
        value: "22500.00",
        rate: { rate: "2625.00000000", inverse: false, source: "workspace", date: "2026-11-12" },
      },
    ],
    landed_costs: [
      {
        id: "lc-1",
        client_component_ref: "c0ffee00-0000-4000-8000-000000000001",
        amount: "1500.00",
        currency: TZS,
        functional_amount: "1500.00",
        rate: { rate: "1", inverse: false, source: "identity", date: "2026-11-12" },
        evidence_ref: "Freight note 44",
        allocations: [{ receipt_line_id: "rl-1", functional_amount: "1500.00" }],
      },
    ],
  })
  purchasing(api, confirmed(), [booked])
  await page.goto("/purchases/po-1/receipts/rc-1")
  const lines = page.getByRole("region", { name: "Lines" })
  await expect(lines).toContainText("TZS 22,500.00")
  await expect(lines).toContainText("Rate 2,625")
  await expect(page.getByRole("region", { name: "Extra costs" })).toContainText("Freight note 44")
  await expect(page.getByRole("button", { name: "Book into stock" })).toHaveCount(0)
  await expectFits(page)
  await expectNoRawValues(page, { allow })
})
