import { expectNoRawValues } from "../../src/test"
import { expect, expectFits, me, test, type FakeApi } from "./fake-api"

// Provider exceptions come only from provider callbacks, which the full stack can't produce
// (accounting.md "Provider observations"); their actions are covered here against a fake API.

const exception = (over: Record<string, unknown> = {}) => ({
  id: "ex-1",
  kind: "pending_posting",
  reason_code: "no_receivable",
  message: "There is no open receivable for this invoice.",
  state: "pending",
  resolution: null,
  attempts: 1,
  last_error: null,
  version: 1,
  created_at: "2026-11-18T09:00:00Z",
  resolved_at: null,
  relation_id: null,
  amount: null,
  recovery_residual: null,
  observation_id: "ob-1",
  provider: "globadyme",
  economic_kind: "collection",
  external_id: "GD-778",
  observed_amount: 1250000,
  currency: "TZS",
  decimals: 2,
  invoice_id: "inv-1",
  credit_note_id: null,
  order_id: null,
  operation_id: "op-1",
  occurred_at: "2026-11-18T08:59:00Z",
  disposition: "pending_observation",
  allowed_actions: ["retry"],
  ...over,
})

const discrepancy = (over: Record<string, unknown> = {}) =>
  exception({
    id: "ex-2",
    kind: "discrepancy",
    reason_code: "excess_refund",
    message: "Refund exceeds the open credit.",
    economic_kind: "refund",
    amount: 200000,
    recovery_residual: 200000,
    relation_id: "rel-1",
    disposition: "posted",
    allowed_actions: ["resolve"],
    ...over,
  })

function book(api: FakeApi, rows: ReturnType<typeof exception>[]) {
  const state = { rows }
  api
    .on("GET", "/workspace/:ws/accounting/exceptions", ({ query }) => ({
      data: state.rows.filter(
        (r) => query.get("state") === "all" || r.state === "pending"
      ),
    }))
    .on("GET", "/workspace/:ws/accounting/exceptions/:id", ({ params }) => {
      const r = state.rows.find((x) => x.id === params[1])
      return r ? { data: r } : { status: 404, body: { message: "Not found." } }
    })
    .on("GET", "/workspace/:ws/relations", () => ({
      data: [
        {
          id: "rel-1",
          company_name: "Mama Asha Shop",
          alias: null,
          relation_nr: "C-1",
        },
      ],
      meta: {
        current_page: 1,
        last_page: 1,
        per_page: 100,
        total: 1,
        from: 1,
        to: 1,
      },
    }))
  return state
}

test("lists what waits, in words, and filters to all", async ({
  page,
  api,
}) => {
  book(api, [
    exception(),
    discrepancy({
      state: "resolved",
      resolution: "write_off",
      allowed_actions: [],
    }),
  ])
  await page.goto("/accounting/exceptions")
  await expect(
    page.getByRole("heading", { name: "Exceptions", exact: true })
  ).toBeVisible()
  const sections = page.getByRole("navigation", { name: "Accounting sections" })
  await expect(
    sections.getByRole("link", { name: "Exceptions" })
  ).toHaveAttribute("aria-current", "page")
  await expect(
    page.getByRole("link", { name: "Payment from globadyme" })
  ).toBeVisible()
  await expect(
    page.getByText("There was no open amount to apply this payment to.").first()
  ).toBeVisible()
  await expect(
    page.getByRole("link", { name: "Refund from globadyme" })
  ).toHaveCount(0)
  await page.getByRole("link", { name: "All", exact: true }).click()
  await expect(
    page.getByRole("link", { name: "Refund from globadyme" })
  ).toBeVisible()
  await expectNoRawValues(page)
  await expectFits(page)
})

test("nothing waiting says so", async ({ page, api }) => {
  book(api, [])
  await page.goto("/accounting/exceptions")
  await expect(
    page.getByText("Nothing waiting. All provider money is booked.")
  ).toBeVisible()
  await expectFits(page)
})

test.describe("retry", () => {
  test.use({
    allow: {
      responses: [
        {
          url: "/v1/workspace/w1/accounting/exceptions/ex-1/retry",
          status: 422,
        },
      ],
    },
  })
  test("a retry that still fails shows why; once fixed it posts", async ({
    page,
    api,
  }) => {
    const state = book(api, [exception()])
    let fixed = false
    api.on("POST", "/workspace/:ws/accounting/exceptions/:id/retry", () => {
      if (!fixed)
        return {
          status: 422,
          body: {
            message: "This invoice has no open receivable yet.",
            code: "posting_failed",
          },
        }
      state.rows = [
        exception({
          state: "resolved",
          resolution: "posted",
          disposition: "posted",
          allowed_actions: [],
        }),
      ]
      return { data: state.rows[0] }
    })
    await page.goto("/accounting/exceptions/ex-1")
    const next = page.getByRole("region", { name: "Next step" })
    await expect(next).toContainText(
      "This money is received but not in the books yet."
    )
    await next.getByRole("button", { name: "Try posting again" }).click()
    let dialog = page.getByRole("dialog")
    await dialog.getByRole("button", { name: "Try posting again" }).click()
    await expect(dialog).toContainText(
      "This invoice has no open receivable yet."
    )
    await dialog.getByRole("button", { name: "Cancel" }).click()

    fixed = true
    await next.getByRole("button", { name: "Try posting again" }).click()
    dialog = page.getByRole("dialog")
    await dialog.getByRole("button", { name: "Try posting again" }).click()
    await expect(
      next.getByRole("status").filter({ hasText: "Payment posted." })
    ).toBeVisible()
    await expect(next).toContainText("Done: posted.")
    const sent = api.callsTo(
      "POST",
      "/workspace/:ws/accounting/exceptions/:id/retry"
    )
    expect(sent).toHaveLength(2)
    expect(sent[1]?.body).toMatchObject({ expected_version: 1, payload: {} })
    expect(sent[0]?.headers["idempotency-key"]).not.toBe(
      sent[1]?.headers["idempotency-key"]
    )
    await expectFits(page)
  })
})

test("a discrepancy is written off after the server's review", async ({
  page,
  api,
}) => {
  const state = book(api, [discrepancy()])
  api.on(
    "POST",
    "/workspace/:ws/accounting/exceptions/:id/resolve/review",
    ({ body }) => ({
      data: {
        resolution: (body as { resolution: string }).resolution,
        amount: "2000.00",
        accounting_date: "2026-11-19",
        debit: "financial_loss",
        credit: "financial_discrepancy",
        relation_id: null,
        review_fingerprint: "fp-1",
      },
    })
  )
  api.on("POST", "/workspace/:ws/accounting/exceptions/:id/resolve", () => {
    state.rows = [
      discrepancy({
        state: "resolved",
        resolution: "write_off",
        recovery_residual: 0,
        allowed_actions: [],
      }),
    ]
    return { data: state.rows[0] }
  })
  await page.goto("/accounting/exceptions/ex-2")
  await expect(
    page.getByText("The refund was more than the customer had paid.").first()
  ).toBeVisible()
  const next = page.getByRole("region", { name: "Next step" })
  await next.getByRole("button", { name: "Resolve" }).click()
  const dialog = page.getByRole("dialog")
  await dialog.getByLabel("Write off as a loss").check()
  await dialog.getByLabel("Reason").fill("Card scheme loss")
  await dialog.getByLabel("Evidence reference").fill("case-7")
  await dialog.getByRole("button", { name: "Review" }).click()
  await expect(dialog).toContainText("Money written off")
  await dialog.getByRole("button", { name: "Resolve" }).click()
  await expect(
    next.getByRole("status").filter({ hasText: "Difference resolved." })
  ).toBeVisible()
  const sent = api.callsTo(
    "POST",
    "/workspace/:ws/accounting/exceptions/:id/resolve"
  )[0]
  expect(sent?.body).toMatchObject({
    expected_version: 1,
    payload: {
      resolution: "write_off",
      reason: "Card scheme loss",
      evidence_ref: "case-7",
      review_fingerprint: "fp-1",
    },
  })
  await expectNoRawValues(page)
  await expectFits(page)
})

test.describe("stale", () => {
  test.use({
    allow: {
      responses: [
        {
          url: "/v1/workspace/w1/accounting/exceptions/ex-2/resolve",
          status: 409,
        },
      ],
    },
  })
  test("a changed exception is reloaded and said so", async ({ page, api }) => {
    book(api, [discrepancy()])
    api.on(
      "POST",
      "/workspace/:ws/accounting/exceptions/:id/resolve/review",
      () => ({
        data: {
          resolution: "write_off",
          amount: "2000.00",
          accounting_date: "2026-11-19",
          debit: "financial_loss",
          credit: "financial_discrepancy",
          relation_id: null,
          review_fingerprint: "fp-1",
        },
      })
    )
    api.on("POST", "/workspace/:ws/accounting/exceptions/:id/resolve", () => ({
      status: 409,
      body: { message: "Changed.", code: "stale_document" },
    }))
    await page.goto("/accounting/exceptions/ex-2")
    await page
      .getByRole("region", { name: "Next step" })
      .getByRole("button", { name: "Resolve" })
      .click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Write off as a loss").check()
    await dialog.getByLabel("Reason").fill("x reason")
    await dialog.getByLabel("Evidence reference").fill("e-1")
    await dialog.getByRole("button", { name: "Review" }).click()
    await dialog.getByRole("button", { name: "Resolve" }).click()
    await expect(
      page.getByText("This changed since you opened it.").first()
    ).toBeVisible()
  })
})

test("a reader sees the exception but no action", async ({ page, api }) => {
  api.on("GET", "/me", () => ({
    data: {
      user: {
        ...me,
        permissions: [{ workspace: "w1", permissions: ["accounting.read"] }],
      },
    },
  }))
  book(api, [exception()])
  await page.goto("/accounting/exceptions/ex-1")
  await expect(
    page.getByRole("heading", { name: "Payment from globadyme" })
  ).toBeVisible()
  await expect(
    page.getByRole("button", { name: "Try posting again" })
  ).toHaveCount(0)
  await expect(page.getByText("Nothing to do for you here.")).toBeVisible()
})
