import { expect, paged, test, type FakeApi } from "./fake-api"

type Order = {
  id: string
  number: string
  status: string
  paid_at: string | null
  invoice_id: string | null
  allowed_actions?: string[]
  [k: string]: unknown
}

// What the API would allow per status; a test can pass its own allowed_actions instead.
function allowedFor(o: Order): string[] {
  const byStatus: Record<string, string[]> = {
    draft: ["confirm", "cancel"],
    awaiting_payment: ["cancel"],
    open: ["pick", "complete", "invoice", "cancel"],
    completed: ["return", "invoice"],
    partially_returned: ["return", "invoice"],
  }
  return [
    ...(byStatus[o.status] ?? []),
    ...(o.paid_at ? ["refund"] : []),
  ].filter((a) => a !== "invoice" || !o.invoice_id)
}

function ordersApi(api: FakeApi, start: Partial<Order> = {}) {
  const order: Order = {
    id: "ord-1",
    number: "SO-1001",
    status: "draft",
    source: "storefront",
    relation_id: "rel-1",
    currency_id: "cur-usd",
    currency: "USD",
    decimals: 2,
    warehouse_id: "wh-1",
    customer: { name: "Kim Lee", email: "kim@example.com" },
    reference: null,
    note: null,
    subtotal: 10000,
    vat: 2100,
    total: 12100,
    summary: { vats: [] },
    expires_at: null,
    paid_at: null,
    completed_at: null,
    cancelled_at: null,
    invoice_id: null,
    lines: [
      {
        id: "line-1",
        product_id: "prod-honey",
        product_variant_id: null,
        title: "AFM Honey 500g",
        quantity: "8",
        unit_price: 1000,
        vat_rate: "21",
        discount_percentage: "0",
        line_excl: 8000,
        line_vat: 1680,
        tracked: true,
        picked: "0",
        deducted: "0",
        returned: "0",
      },
      {
        id: "line-2",
        product_id: "prod-wax",
        product_variant_id: null,
        title: "Beeswax Block",
        quantity: "2",
        unit_price: 1000,
        vat_rate: "21",
        discount_percentage: "0",
        line_excl: 2000,
        line_vat: 420,
        tracked: true,
        picked: "0",
        deducted: "0",
        returned: "0",
      },
    ],
    created_at: "2026-10-30T09:00:00Z",
    updated_at: "2026-10-30T09:00:00Z",
    ...start,
  }
  if (!start.allowed_actions) order.allowed_actions = allowedFor(order)
  const to = (status: string) => () => {
    order.status = status
    order.allowed_actions = allowedFor(order)
    return { data: order }
  }
  api
    .on("GET", "/workspace/:ws/orders", () => paged([order]))
    .on("GET", "/workspace/:ws/orders/:id", () => ({ data: order }))
    .on("POST", "/workspace/:ws/orders/:id/confirm", to("open"))
    .on("POST", "/workspace/:ws/orders/:id/complete", to("completed"))
    .on("POST", "/workspace/:ws/orders/:id/cancel", () =>
      order.paid_at
        ? {
            status: 409,
            body: {
              message: "This order is paid: refund it first.",
              code: "refund_required",
            },
          }
        : to("cancelled")()
    )
    .on("POST", "/workspace/:ws/orders/:id/pick", ({ body }) => {
      const { lines } = body as {
        lines: { line_id: string; quantity: string }[]
      }
      for (const l of lines)
        (order.lines as { id: string; picked: string }[]).find(
          (x) => x.id === l.line_id
        )!.picked = l.quantity
      return { data: order }
    })
    .on("POST", "/workspace/:ws/orders/:id/returns", to("partially_returned"))
    .on("POST", "/workspace/:ws/orders/:id/invoice", () => {
      order.invoice_id = "inv-9"
      order.allowed_actions = allowedFor(order)
      return { data: order }
    })
    .on("POST", "/workspace/:ws/orders/:id/refunds", ({ body }) => ({
      status: 201,
      body: {
        data: {
          refund_id: "rf-1",
          amount: (body as { amount: number }).amount,
          status: "creating",
        },
      },
    }))
  return order
}

test("list shows money from minor units and filters by status", async ({
  page,
  api,
}) => {
  ordersApi(api)
  await page.goto("/orders")
  await expect(page.getByRole("cell", { name: "$121.00" })).toBeVisible()
  await expect(page.getByRole("cell", { name: "Kim Lee" })).toBeVisible()
  await page.getByRole("combobox", { name: "Status" }).selectOption("open")
  await expect(page).toHaveURL(/status=open/)
  await page.getByRole("link", { name: "SO-1001" }).click()
  await expect(page.getByRole("heading", { name: "SO-1001" })).toBeVisible()
  await expect(page.getByRole("cell", { name: "$80.00" })).toBeVisible()
})

test("confirm a draft, pick it (cancel sends nothing), complete it", async ({
  page,
  api,
}) => {
  ordersApi(api)
  await page.goto("/orders/ord-1")
  await page.getByRole("button", { name: "Confirm order" }).click()
  await expect(page.getByText("Open", { exact: true })).toBeVisible()
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Order confirmed; stock is reserved." })
  ).toBeVisible()
  const confirmCall = api.callsTo(
    "POST",
    "/workspace/:ws/orders/:id/confirm"
  )[0]
  expect(confirmCall?.headers["idempotency-key"]).toMatch(/^[0-9a-f-]{36}$/)

  await page.getByRole("button", { name: "Record picking" }).click()
  await page.getByRole("dialog").getByRole("button", { name: "Cancel" }).click()
  await expect(page.getByRole("dialog")).toBeHidden()
  expect(api.callsTo("POST", "/workspace/:ws/orders/:id/pick")).toHaveLength(0)

  await page.getByRole("button", { name: "Record picking" }).click()
  await page.getByRole("dialog").getByLabel("Beeswax Block (2)").fill("1")
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Record picking" })
    .click()
  await expect(page.getByRole("dialog")).toBeHidden()
  expect(
    api.callsTo("POST", "/workspace/:ws/orders/:id/pick")[0]?.body
  ).toEqual({
    lines: [
      { line_id: "line-1", quantity: "8" },
      { line_id: "line-2", quantity: "1" },
    ],
  })

  await page.getByRole("button", { name: "Complete" }).click()
  await page.getByRole("button", { name: "Cancel", exact: true }).click()
  await expect(page.getByRole("alertdialog")).toBeHidden()
  expect(
    api.callsTo("POST", "/workspace/:ws/orders/:id/complete")
  ).toHaveLength(0)
  await page.getByRole("button", { name: "Complete" }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Complete" })
    .click()
  await expect(page.getByText("Completed", { exact: true })).toBeVisible()
})

test("cancel: closing the dialog sends nothing; confirming cancels", async ({
  page,
  api,
}) => {
  ordersApi(api, { status: "open" })
  await page.goto("/orders/ord-1")
  await page.getByRole("button", { name: "Cancel order" }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Cancel", exact: true })
    .click()
  await expect(page.getByRole("alertdialog")).toBeHidden()
  expect(api.callsTo("POST", "/workspace/:ws/orders/:id/cancel")).toHaveLength(
    0
  )

  await page.getByRole("button", { name: "Cancel order" }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Cancel order" })
    .click()
  await expect(page.getByText("Cancelled", { exact: true })).toBeVisible()
})

test.describe(() => {
  test.use({
    allow: {
      responses: [{ url: "/v1/workspace/w1/orders/ord-1/cancel", status: 409 }],
    },
  })

  test("cancelling a paid order shows the API's refund_required answer", async ({
    page,
    api,
  }) => {
    // The list said cancel was allowed, then a payment landed: the API still answers 409.
    ordersApi(api, {
      status: "open",
      paid_at: "2026-10-30T10:00:00Z",
      allowed_actions: ["cancel", "refund"],
    })
    await page.goto("/orders/ord-1")
    await page.getByRole("button", { name: "Cancel order" }).click()
    await page
      .getByRole("alertdialog")
      .getByRole("button", { name: "Cancel order" })
      .click()
    // Next's route announcer is also role=alert; pick ours by its text.
    await expect(
      page
        .getByRole("alert")
        .filter({ hasText: "This order is paid: refund it first." })
    ).toBeVisible()
  })
})

test("refund: cancel sends nothing; 12.50 goes out as 1250 minor units", async ({
  page,
  api,
}) => {
  ordersApi(api, { status: "completed", paid_at: "2026-10-30T10:00:00Z" })
  await page.goto("/orders/ord-1")
  await page.getByRole("button", { name: "Refund" }).click()
  await page.getByRole("dialog").getByLabel("Amount (USD)").fill("12.50")
  await page.getByRole("dialog").getByRole("button", { name: "Cancel" }).click()
  await expect(page.getByRole("dialog")).toBeHidden()
  expect(api.callsTo("POST", "/workspace/:ws/orders/:id/refunds")).toHaveLength(
    0
  )

  await page.getByRole("button", { name: "Refund" }).click()
  const dialog = page.getByRole("dialog")
  await dialog.getByLabel("Amount (USD)").fill("12.505")
  await expect(dialog.getByText("Enter an amount like 12.50.")).toBeVisible()
  await expect(dialog.getByRole("button", { name: "Refund" })).toBeDisabled()
  await dialog.getByLabel("Amount (USD)").fill("12.50")
  await dialog.getByRole("button", { name: "Refund" }).click()
  await expect(dialog.getByRole("status")).toContainText("$12.50")
  expect(
    api.callsTo("POST", "/workspace/:ws/orders/:id/refunds")[0]?.body
  ).toEqual({
    amount: 1250,
  })
})

test("returns send only the lines filled in; invoice links once made", async ({
  page,
  api,
}) => {
  ordersApi(api, { status: "completed" })
  await page.goto("/orders/ord-1")
  await page.getByRole("button", { name: "Take back returns" }).click()
  await page.getByRole("dialog").getByLabel("AFM Honey 500g (8)").fill("1")
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Take back returns" })
    .click()
  await expect(page.getByText("Partly returned", { exact: true })).toBeVisible()
  expect(
    api.callsTo("POST", "/workspace/:ws/orders/:id/returns")[0]?.body
  ).toEqual({
    lines: [{ line_id: "line-1", quantity: "1" }],
    restock: true,
  })

  await page.getByRole("button", { name: "Make invoice" }).click()
  await expect(
    page.getByRole("link", { name: "Open invoice" })
  ).toHaveAttribute("href", "/invoices/inv-9")
})

test("refund retried after a network failure reuses its key; the API sees one refund", async ({
  page,
  api,
}) => {
  ordersApi(api, { status: "completed", paid_at: "2026-10-30T10:00:00Z" })
  const sent: string[] = []
  let fail = true
  // Before the fake API: the first refund call dies on the wire, later ones go through.
  await page.route("**/v1/workspace/w1/orders/ord-1/refunds", async (route) => {
    sent.push(route.request().headers()["idempotency-key"] ?? "")
    if (fail) {
      fail = false
      return route.abort("aborted")
    }
    return route.fallback()
  })
  await page.goto("/orders/ord-1")
  await page.getByRole("button", { name: "Refund" }).click()
  const dialog = page.getByRole("dialog")
  await dialog.getByLabel("Amount (USD)").fill("12.50")
  await dialog.getByRole("button", { name: "Refund" }).click()
  await expect(
    dialog
      .getByRole("alert")
      .filter({ hasText: "Something went wrong. Try again." })
  ).toBeVisible()

  await dialog.getByRole("button", { name: "Refund" }).click()
  await expect(dialog.getByRole("status")).toContainText("$12.50")
  expect(sent).toHaveLength(2)
  expect(sent[0]).toMatch(/^[0-9a-f-]{36}$/)
  expect(sent[1]).toBe(sent[0])
  const refunds = api.callsTo("POST", "/workspace/:ws/orders/:id/refunds")
  expect(refunds).toHaveLength(1)
  expect(refunds[0]?.headers["idempotency-key"]).toBe(sent[0])

  // A new refund is a new attempt: new key.
  await page.keyboard.press("Escape")
  await expect(dialog).toBeHidden()
  await page.getByRole("button", { name: "Refund" }).click()
  await dialog.getByLabel("Amount (USD)").fill("1")
  await dialog.getByRole("button", { name: "Refund" }).click()
  await expect(dialog.getByRole("status")).toContainText("$1.00")
  expect(sent).toHaveLength(3)
  expect(sent[2]).not.toBe(sent[0])
})

test("only the API's allowed_actions show as buttons", async ({
  page,
  api,
}) => {
  ordersApi(api, {
    status: "open",
    paid_at: "2026-10-30T10:00:00Z",
    allowed_actions: ["refund"],
  })
  await page.goto("/orders/ord-1")
  await expect(page.getByRole("button", { name: "Refund" })).toBeVisible()
  for (const name of [
    "Confirm order",
    "Record picking",
    "Complete",
    "Make invoice",
    "Take back returns",
    "Cancel order",
  ])
    await expect(page.getByRole("button", { name, exact: true })).toHaveCount(0)
})

test.describe(() => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/orders/ord-1/refunds", status: 409 },
      ],
    },
  })

  test("refund on a fully refunded order shows the API's 409", async ({
    page,
    api,
  }) => {
    ordersApi(api, { status: "completed", paid_at: "2026-10-30T10:00:00Z" })
    api.on("POST", "/workspace/:ws/orders/:id/refunds", () => ({
      status: 409,
      body: { message: "Nothing paid on this order is left to refund." },
    }))
    await page.goto("/orders/ord-1")
    await page.getByRole("button", { name: "Refund" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Amount (USD)").fill("1")
    await dialog.getByRole("button", { name: "Refund" }).click()
    await expect(
      dialog
        .getByRole("alert")
        .filter({ hasText: "Nothing paid on this order is left to refund." })
    ).toBeVisible()
  })
})
