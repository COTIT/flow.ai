import type { Page } from "@playwright/test"

import { choose, expect, expectFits, type FakeApi, paged, test } from "./fake-api"

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
        lots: [] as { id: string; code: string; quantity: string }[],
        image: null as { url: string; expires_at: string } | null,
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
        lots: [] as { id: string; code: string; quantity: string }[],
        image: null as { url: string; expires_at: string } | null,
      },
    ],
    units: "10.00",
    payment_method: null,
    confirmed_at: null,
    picked_at: null,
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
  await choose(page, "Status", "Open")
  await expect(page).toHaveURL(/status=open/)
  await page.getByRole("link", { name: "SO-1001" }).click()
  await expect(page.getByRole("heading", { name: "SO-1001" })).toBeVisible()
  await expect(page.getByRole("cell", { name: "$80.00" })).toBeVisible()
  // OfficeOrder tiles: server totals at the foot of Lines, the main action big in Next step.
  await expect(page.getByRole("region", { name: "Lines" })).toContainText(
    "Amounts as calculated by the server."
  )
  await expect(
    page
      .getByRole("region", { name: "Next step" })
      .getByRole("button", { name: "Confirm order" })
  ).toBeVisible()
  await expect(page.getByRole("region", { name: "Customer" })).toContainText(
    "kim@example.com"
  )
  await expect(page.getByRole("region", { name: "Timeline" })).toContainText(
    "Placed"
  )
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
  await expectFits(page)
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

// A data URL keeps the picture inside the office CSP in tests (no storage host here).
const pixel = "data:image/gif;base64,R0lGODlhAQABAAAAACw="

type Line = {
  lots: { id: string; code: string; quantity: string }[]
  image: { url: string; expires_at: string } | null
}

test("lines: picture, lot codes, and the API's item and unit counts", async ({
  page,
  api,
}) => {
  const order = ordersApi(api, { status: "open", units: "10.00" })
  const [honey, wax] = order.lines as Line[]
  honey!.image = { url: pixel, expires_at: "2099-01-01T00:00:00Z" }
  honey!.lots = [
    { id: "lot-a", code: "AFM-HN-2026-07", quantity: "5.00" },
    { id: "lot-b", code: "AFM-HN-2026-08", quantity: "3.00" },
  ]
  wax!.lots = []
  await page.goto("/orders/ord-1")
  const lines = page.getByRole("region", { name: "Lines" })
  await expect(lines).toContainText("2 items · 10 units")
  const honeyRow = lines.getByRole("row").filter({ hasText: "AFM Honey 500g" })
  await expect(honeyRow.getByText("Lot AFM-HN-2026-07 · Lot AFM-HN-2026-08")).toBeVisible()
  await expect(honeyRow.locator(`img[src="${pixel}"]`)).toHaveCount(1)
  const waxRow = lines.getByRole("row").filter({ hasText: "Beeswax Block" })
  await expect(waxRow.locator("img")).toHaveCount(0)
  await expect(waxRow.getByText(/^Lot /)).toHaveCount(0)
  await expectFits(page)
})

test("payment: paid with its method, paid without one, or not paid", async ({ page, api }) => {
  const order = ordersApi(api, {
    status: "completed",
    paid_at: "2026-10-30T10:00:00Z",
    payment_method: "card",
  })
  await page.goto("/orders/ord-1")
  const customer = page.getByRole("region", { name: "Customer" })
  await expect(customer.getByText("Paid · card")).toBeVisible()

  order.payment_method = null
  await page.reload()
  await expect(customer.getByText("Paid", { exact: true })).toBeVisible()

  order.paid_at = null
  await page.reload()
  await expect(customer.getByText("Not paid")).toBeVisible()
})

test("timeline: the order's trail, oldest first, with who did it", async ({ page, api }) => {
  ordersApi(api, { status: "open", paid_at: "2026-10-30T09:01:00Z" })
  const event = (id: string, kind: string, at: string, actor: object | null) => ({
    id,
    kind,
    at,
    actor,
    meta: null,
  })
  // The API answers newest first.
  api.on("GET", "/workspace/:ws/orders/:id/events", () =>
    paged([
      event("e4", "picked", "2026-10-30T09:30:00Z", { type: "user", id: "u1", name: "Romeo T." }),
      event("e3", "confirmed", "2026-10-30T08:55:00Z", { type: "user", id: "u1", name: "Romeo T." }),
      event("e2", "paid", "2026-10-30T08:41:00Z", null),
      event("e1", "created", "2026-10-30T08:40:00Z", { type: "user", id: "u2", name: null }),
    ])
  )
  await page.goto("/orders/ord-1")
  const timeline = page.getByRole("region", { name: "Timeline" })
  const steps = timeline.getByRole("listitem")
  await expect(steps).toHaveCount(4)
  await expect(steps.nth(0)).toContainText("Placed")
  // A step with no actor (the payment provider) or one who left shows its time only.
  await expect(steps.nth(0)).not.toContainText("·")
  await expect(steps.nth(1)).toContainText("Paid")
  await expect(steps.nth(1)).not.toContainText("·")
  await expect(steps.nth(2)).toContainText("Confirmed")
  await expect(steps.nth(2)).toContainText("· Romeo T.")
  await expect(steps.nth(3)).toContainText("Picked")
  await expect(steps.nth(3)).toContainText("· Romeo T.")
  await expect(timeline.getByText(/recorded from now on/)).toHaveCount(0)
  await expectFits(page)
})

test("timeline: an older order without a trail shows its own times and says why", async ({
  page,
  api,
}) => {
  ordersApi(api, {
    status: "open",
    paid_at: "2026-10-30T09:05:00Z",
    confirmed_at: "2026-10-30T09:10:00Z",
    picked_at: "2026-10-30T10:00:00Z",
  })
  await page.goto("/orders/ord-1")
  const timeline = page.getByRole("region", { name: "Timeline" })
  const steps = timeline.getByRole("listitem")
  await expect(steps).toHaveCount(4)
  await expect(steps.nth(0)).toContainText("Placed")
  await expect(steps.nth(1)).toContainText("Paid")
  await expect(steps.nth(2)).toContainText("Confirmed")
  await expect(steps.nth(3)).toContainText("Picked")
  await expect(
    timeline.getByText("Who did what is recorded from now on; older steps show their times only.")
  ).toBeVisible()
})

// Asks first: open the question (once the last one has closed), then answer it.
async function askThen(page: Page, opener: string, action: string) {
  const ask = page.getByRole("alertdialog")
  await expect(ask).toBeHidden()
  await page.getByRole("main").getByRole("button", { name: opener }).click()
  await ask.getByRole("button", { name: action }).click()
}

// Every order action, how to start it, and the order state that offers it.
const orderActions: {
  action: string
  path: string
  start: Partial<Order>
  run: (page: Page) => Promise<void>
}[] = [
  {
    action: "confirm",
    path: "confirm",
    start: { status: "draft" },
    run: (page) => page.getByRole("button", { name: "Confirm order" }).click(),
  },
  {
    action: "pick",
    path: "pick",
    start: { status: "open" },
    run: async (page) => {
      await page.getByRole("button", { name: "Record picking" }).click()
      await page
        .getByRole("dialog")
        .getByRole("button", { name: "Record picking" })
        .click()
    },
  },
  {
    action: "complete",
    path: "complete",
    start: { status: "open" },
    run: (page) => askThen(page, "Complete", "Complete"),
  },
  {
    action: "cancel",
    path: "cancel",
    start: { status: "open" },
    run: (page) => askThen(page, "Cancel order", "Cancel order"),
  },
  {
    action: "return",
    path: "returns",
    start: { status: "completed" },
    run: async (page) => {
      await page.getByRole("button", { name: "Take back returns" }).click()
      await page.getByRole("dialog").getByLabel("AFM Honey 500g (8)").fill("1")
      await page
        .getByRole("dialog")
        .getByRole("button", { name: "Take back returns" })
        .click()
    },
  },
  {
    action: "invoice",
    path: "invoice",
    start: { status: "completed" },
    run: (page) => page.getByRole("button", { name: "Make invoice" }).click(),
  },
  {
    action: "refund",
    path: "refunds",
    start: { status: "completed", paid_at: "2026-10-30T10:00:00Z" },
    run: async (page) => {
      await page.getByRole("button", { name: "Refund" }).click()
      await page.getByRole("dialog").getByLabel("Amount (USD)").fill("1")
      await page
        .getByRole("dialog")
        .getByRole("button", { name: "Refund" })
        .click()
    },
  },
]

// A refusal the API gives for a stale order (another user moved it on) or a missing right.
const refusals = [
  { status: 403, message: "You may not change orders in this workspace." },
  { status: 409, message: "This order was changed by someone else. Reload it." },
  { status: 500, message: "The server could not finish this." },
]

for (const { action, path, start, run } of orderActions)
  for (const { status, message } of refusals)
    test.describe(() => {
      test.use({
        allow: {
          responses: [
            { url: `/v1/workspace/w1/orders/ord-1/${path}`, status },
          ],
        },
      })

      test(`orders/[id] ${action}: the API's ${status} shows, sent once, nothing marked done`, async ({
        page,
        api,
      }) => {
        ordersApi(api, start)
        api.on("POST", `/workspace/:ws/orders/:id/${path}`, () => ({
          status,
          body: { message },
        }))
        await page.goto("/orders/ord-1")
        await expect(page.getByRole("heading", { name: "SO-1001" })).toBeVisible()
        const reads = api.callsTo("GET", "/workspace/:ws/orders/:id").length
        await run(page)
        await expect(
          page.getByRole("alert").filter({ hasText: message }),
          `orders/[id] ${action} ${status}`
        ).toBeVisible()
        // A write is never retried by itself, and a refusal does not reload the order as if done.
        expect(api.callsTo("POST", `/workspace/:ws/orders/:id/${path}`)).toHaveLength(1)
        expect(api.callsTo("GET", "/workspace/:ws/orders/:id")).toHaveLength(reads)
        // The returns dialog's restock box is checked on its own below.
        if (action !== "return") await expectFits(page)
      })
    })

for (const { action, path, start, run } of orderActions.filter((a) =>
  ["confirm", "complete", "cancel", "invoice"].includes(a.action)
))
  test(`orders/[id] ${action}: retried after a network failure, the key is reused; the API sees it once`, async ({
    page,
    api,
  }) => {
    ordersApi(api, start)
    const keys: string[] = []
    let fail = true
    await page.route(`**/v1/workspace/w1/orders/ord-1/${path}`, async (route) => {
      keys.push(route.request().headers()["idempotency-key"] ?? "")
      if (fail) {
        fail = false
        return route.abort("aborted")
      }
      return route.fallback()
    })
    await page.goto("/orders/ord-1")
    await run(page)
    await expect(
      page.getByRole("alert").filter({ hasText: "Something went wrong. Try again." }),
      `orders/[id] ${action} network`
    ).toBeVisible()
    await run(page)
    await expect.poll(() => api.callsTo("POST", `/workspace/:ws/orders/:id/${path}`).length).toBe(1)
    expect(keys).toHaveLength(2)
    expect(keys[0]).toMatch(/^[0-9a-f-]{36}$/)
    expect(keys[1], `orders/[id] ${action} retry key`).toBe(keys[0])
  })

test("orders/[id] returns dialog: every control is a 44 px target on a phone", async ({
  page,
  api,
}) => {
  test.fail(
    (page.viewportSize()?.width ?? 0) < 768,
    "BUG: the Restock box is a bare 13x13 <input type=checkbox> (apps/office/app/(app)/orders/[id]/page.tsx:789), not the 44 px Checkbox"
  )
  ordersApi(api, { status: "completed" })
  await page.goto("/orders/ord-1")
  await page.getByRole("button", { name: "Take back returns" }).click()
  await expect(page.getByRole("dialog").getByLabel("Put returned goods back on stock")).toBeChecked()
  await expectFits(page)
})
