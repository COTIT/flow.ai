import { expectNoRawValues, pick } from "../../src/test"
import {
  expect,
  expectFits,
  me,
  openMenu,
  paged,
  test,
  type FakeApi,
} from "./fake-api"

const TZS = { id: "cur-tzs", code: "TZS", decimals: 2 }

const account = (
  id: string,
  code: string,
  name: string,
  type: string,
  over: Record<string, unknown> = {}
) => ({
  id,
  workspace_id: "w1",
  code,
  name,
  type,
  parent_id: null,
  active: true,
  roles: [] as string[],
  version: 1,
  allowed_actions: ["update"],
  created_by: "u1",
  created_at: "2026-11-01T08:00:00Z",
  updated_at: "2026-11-01T08:00:00Z",
  ...over,
})

const setup = (over: Record<string, unknown> = {}) => ({
  state: "setup",
  version: 3,
  functional_currency: TZS,
  timezone: "Africa/Dar_es_Salaam",
  business_date: "2026-11-15",
  cutover_at: null,
  activated_at: null,
  account_mappings: { cash: "a-cash" },
  missing_roles: ["revenue", "receivables"],
  configured: false,
  allowed_actions: ["update"],
  ...over,
})

const line = (
  no: number,
  accountId: string,
  code: string,
  debit: string,
  credit: string
) => ({
  line_no: no,
  account_id: accountId,
  account_code: code,
  relation_id: null,
  source_ref: null,
  debit,
  credit,
  functional_debit: debit,
  functional_credit: credit,
  rate: "1",
  rate_inverse: false,
  rate_source: "identity",
  rate_date: null,
})

const journal = (over: Record<string, unknown> = {}) => ({
  id: "j-1",
  workspace_id: "w1",
  number: null,
  kind: "manual",
  state: "draft",
  accounting_date: "2026-11-15",
  currency: TZS,
  functional_currency: null,
  reason: "Accrue November rent",
  evidence_ref: "RENT-11",
  reversal_of: null,
  reversed_by: null,
  period_id: null,
  posted_by: null,
  posted_at: null,
  version: 1,
  totals: {
    debit: "150000.00",
    credit: "150000.00",
    functional_debit: "0.00",
    functional_credit: "0.00",
  },
  lines: [
    line(1, "a-rent", "6100", "150000.00", "0.00"),
    line(2, "a-accr", "2400", "0.00", "150000.00"),
  ],
  allowed_actions: ["update", "post"],
  created_by: "u1",
  created_at: "2026-11-15T08:00:00Z",
  updated_at: "2026-11-15T08:00:00Z",
  ...over,
})

const posted = (over: Record<string, unknown> = {}) =>
  journal({
    number: 12,
    state: "posted",
    version: 2,
    functional_currency: TZS,
    period_id: "p-11",
    posted_by: "u1",
    posted_at: "2026-11-15T09:00:00Z",
    totals: {
      debit: "150000.00",
      credit: "150000.00",
      functional_debit: "150000.00",
      functional_credit: "150000.00",
    },
    allowed_actions: ["reverse"],
    ...over,
  })

/** A small book: chart, setup, currencies, journals, years and periods. */
function book(
  api: FakeApi,
  over: {
    setup?: ReturnType<typeof setup>
    journals?: ReturnType<typeof journal>[]
  } = {}
) {
  const state = {
    setup: over.setup ?? setup(),
    accounts: [
      account("a-cash", "1000", "Cash in tills", "asset", { roles: ["cash"] }),
      account("a-accr", "2400", "Accrued costs", "liability"),
      account("a-sales", "4000", "Honey sales", "income"),
      account("a-rent", "6100", "Rent", "expense"),
    ],
    journals: over.journals ?? [journal()],
    years: [] as Record<string, unknown>[],
    periods: [] as Record<string, unknown>[],
  }
  api
    .on("GET", "/workspace/:ws/accounting/setup", () => ({ data: state.setup }))
    .on("GET", "/workspace/:ws/accounting/accounts", () =>
      paged(state.accounts, 1, 100)
    )
    .on("GET", "/workspace/:ws/accounting/accounts/:id", ({ params }) => {
      const a = state.accounts.find((x) => x.id === params[1])
      return a ? { data: a } : { status: 404, body: { message: "Not found." } }
    })
    .on("GET", "/workspace/:ws/accounting/journals", () =>
      paged(state.journals)
    )
    .on("GET", "/workspace/:ws/accounting/journals/:id", ({ params }) => {
      const j = state.journals.find((x) => x.id === params[1])
      return j ? { data: j } : { status: 404, body: { message: "Not found." } }
    })
    .on("GET", "/workspace/:ws/accounting/fiscal-years", () =>
      paged(state.years, 1, 100)
    )
    .on("GET", "/workspace/:ws/accounting/periods", () =>
      paged(state.periods, 1, 100)
    )
    .on("GET", "/workspace/:ws/currencies", () =>
      paged(
        [
          {
            id: "cur-tzs",
            asset_type: "TZS",
            decimals: 2,
            default_currency: true,
          },
        ],
        1,
        100
      )
    )
    .on("GET", "/workspace/:ws/currencies/:id", () => ({
      data: {
        id: "cur-tzs",
        asset_type: "TZS",
        decimals: 2,
        default_currency: true,
      },
    }))
  return state
}

test("the menu shows one Accounting entry with its sections as pills", async ({
  page,
  api,
}) => {
  book(api)
  await page.goto("/accounting")
  await expect(page).toHaveURL(/\/accounting\/journals$/)
  await openMenu(page)
  const nav = page.getByRole("navigation", { name: "Main menu" })
  await expect(nav.getByRole("link", { name: "Accounting" })).toHaveAttribute(
    "aria-current",
    "page"
  )
  await expect(nav.getByRole("link", { name: "Taxes" })).toBeVisible()
  if ((page.viewportSize()?.width ?? 0) < 768)
    await page.keyboard.press("Escape")
  const sections = page.getByRole("navigation", { name: "Accounting sections" })
  for (const s of [
    "Journals",
    "Reports",
    "Accounts",
    "Periods",
    "Cash",
    "Banking",
    "Exceptions",
    "Setup",
  ])
    await expect(sections.getByRole("link", { name: s })).toBeVisible()
  await expect(
    sections.getByRole("link", { name: "Journals" })
  ).toHaveAttribute("aria-current", "page")
})

test.describe("a member without accounting rights", () => {
  test("sees no Accounting entry and no page", async ({ page, api }) => {
    api.on("GET", "/me", () => ({
      data: {
        user: {
          ...me,
          permissions: [{ workspace: "w1", permissions: ["product.list"] }],
        },
      },
    }))
    api.on("GET", "/workspace/:ws/products", () => paged([]))
    await page.goto("/accounting/journals")
    await expect(
      page.getByText(/don.t have access|no access/i).first()
    ).toBeVisible()
    await openMenu(page)
    const nav = page.getByRole("navigation", { name: "Main menu" })
    await expect(nav.getByRole("link", { name: "Accounting" })).toHaveCount(0)
    await expect(nav.getByRole("link", { name: "Taxes" })).toHaveCount(0)
  })

  test("a reader sees the chart but no add, edit or post buttons", async ({
    page,
    api,
  }) => {
    api.on("GET", "/me", () => ({
      data: {
        user: {
          ...me,
          permissions: [{ workspace: "w1", permissions: ["accounting.read"] }],
        },
      },
    }))
    book(api)
    await page.goto("/accounting/accounts")
    await expect(page.getByRole("row", { name: /1000/ })).toContainText(
      "Cash in tills"
    )
    await expect(page.getByRole("button", { name: "Add account" })).toHaveCount(
      0
    )
    await expect(page.getByRole("button", { name: "Edit" })).toHaveCount(0)
    await page.goto("/accounting/journals/j-1")
    await expect(
      page.getByRole("heading", { name: "Draft journal" })
    ).toBeVisible()
    await expect(page.getByRole("button", { name: "Post" })).toHaveCount(0)
    await page.goto("/accounting/setup")
    await expect(page.getByText("You can view the setup.")).toBeVisible()
    await expect(page.getByRole("button", { name: "Save setup" })).toHaveCount(
      0
    )
  })
})

test.describe("chart of accounts", () => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/accounting/accounts/a-cash", status: 422 },
      ],
    },
  })

  test("lists accounts with types and roles, adds one, and refuses switching off a mapped one", async ({
    page,
    api,
  }) => {
    const state = book(api)
    api.on("POST", "/workspace/:ws/accounting/accounts", ({ body }) => {
      const p = (
        body as { payload: { code: string; name: string; type: string } }
      ).payload
      const a = account("a-new", p.code, p.name, p.type)
      state.accounts.push(a)
      return { status: 201, body: { data: a } }
    })
    api.on("PATCH", "/workspace/:ws/accounting/accounts/:id", () => ({
      status: 422,
      body: {
        message: "The given data was invalid.",
        errors: {
          "payload.active": [
            "An account mapped to a role can't be deactivated.",
          ],
        },
      },
    }))
    await page.goto("/accounting/accounts")
    const cash = page.getByRole("row", { name: /1000/ })
    await expect(cash).toContainText("Asset")
    await expect(cash).toContainText("Cash")
    await expect(page.getByText("4 accounts")).toBeVisible()
    await expectFits(page)
    await expectNoRawValues(page)

    await page.getByRole("button", { name: "Add account" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Code").fill("4100")
    await dialog.getByLabel("Name").fill("Wax sales")
    await dialog.getByLabel("Type").selectOption("income")
    await expect(dialog.getByLabel("Part of (optional)")).toContainText(
      "4000 Honey sales"
    )
    await dialog.getByRole("button", { name: "Add account" }).click()
    await expect(
      page.getByRole("status").filter({ hasText: "Account 4100 added." })
    ).toBeVisible()
    await expect(page.getByRole("row", { name: /4100/ })).toContainText(
      "Income"
    )
    const sent = api
      .callsTo("POST", "/workspace/:ws/accounting/accounts")
      .at(-1)!
    expect(sent.body).toMatchObject({
      payload: {
        code: "4100",
        name: "Wax sales",
        type: "income",
        parent_id: null,
      },
    })
    expect((sent.body as { client_ref: string }).client_ref).toBe(
      sent.headers["idempotency-key"]
    )

    await cash.getByRole("button", { name: "Edit" }).click()
    await dialog.getByRole("checkbox", { name: "In use" }).click()
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(
      dialog.getByText("An account mapped to a role can't be deactivated.")
    ).toBeVisible()
    expect(
      api.callsTo("PATCH", "/workspace/:ws/accounting/accounts/:id").at(-1)!
        .body
    ).toMatchObject({
      expected_version: 1,
      payload: { active: false },
    })
  })

  test("an empty chart and a failed load say so", async ({ page, api }) => {
    book(api)
    api.on("GET", "/workspace/:ws/accounting/accounts", () => paged([]))
    await page.goto("/accounting/accounts")
    await expect(page.getByText("No accounts yet.")).toBeVisible()
    await expectFits(page)
  })
})

test.describe("load errors", () => {
  test.use({
    allow: {
      responses: [{ url: "/v1/workspace/w1/accounting/journals", status: 500 }],
    },
  })

  test("a failed journal list offers a retry", async ({ page, api }) => {
    book(api)
    api.on("GET", "/workspace/:ws/accounting/journals", () => ({
      status: 500,
      body: { message: "Server error." },
    }))
    await page.goto("/accounting/journals")
    // TanStack retries three times before giving up.
    await expect(page.getByRole("button", { name: "Try again" })).toBeVisible({
      timeout: 20_000,
    })
  })
})

test.describe("fiscal years and periods", () => {
  test.use({
    allow: {
      responses: [{ url: "/v1/workspace/w1/accounting/periods", status: 422 }],
    },
  })

  test("adds a year, then a period, and shows an overlap next to the date", async ({
    page,
    api,
  }) => {
    const state = book(api)
    await page.goto("/accounting/periods")
    await expect(page.getByText("No fiscal years yet.")).toBeVisible()
    await expect(page.getByRole("button", { name: "Add period" })).toHaveCount(
      0
    )
    await expectFits(page)

    api.on("POST", "/workspace/:ws/accounting/fiscal-years", ({ body }) => {
      const y = {
        id: "y-26",
        state: "open",
        version: 1,
        allowed_actions: [],
        ...(body as { payload: object }).payload,
      }
      state.years.push(y)
      return { status: 201, body: { data: y } }
    })
    await page.getByRole("button", { name: "Add fiscal year" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Starts on").fill("2026-01-01")
    await dialog.getByLabel("Ends before").fill("2027-01-01")
    await dialog.getByRole("button", { name: "Add year" }).click()
    await expect(
      page.getByRole("status").filter({ hasText: "Fiscal year added." })
    ).toBeVisible()
    await expect(page.getByText("No periods in this year yet.")).toBeVisible()

    let tries = 0
    api.on("POST", "/workspace/:ws/accounting/periods", ({ body }) => {
      tries += 1
      if (tries === 1)
        return {
          status: 422,
          body: {
            message: "The given data was invalid.",
            errors: { "payload.start_date": ["Overlaps another period."] },
          },
        }
      const p = {
        id: "p-11",
        state: "open",
        version: 1,
        allowed_actions: [],
        ...(body as { payload: object }).payload,
      }
      state.periods.push(p)
      return { status: 201, body: { data: p } }
    })
    await page.getByRole("button", { name: "Add period" }).click()
    await dialog.getByLabel("Starts on").fill("2026-11-01")
    await dialog.getByLabel("Ends before").fill("2026-12-01")
    await dialog.getByRole("button", { name: "Add period" }).click()
    await expect(dialog.getByText("Overlaps another period.")).toBeVisible()
    await dialog.getByRole("button", { name: "Add period" }).click()
    await expect(
      page.getByRole("status").filter({ hasText: "Period added." })
    ).toBeVisible()
    await expect(page.getByRole("cell", { name: "Open" })).toBeVisible()
    await expectNoRawValues(page)
    expect(
      api.callsTo("POST", "/workspace/:ws/accounting/periods").at(-1)!.body
    ).toMatchObject({
      payload: {
        fiscal_year_id: "y-26",
        start_date: "2026-11-01",
        end_date: "2026-12-01",
      },
    })
  })
})

test.describe("setup", () => {
  test("saves currency, timezone and a role's account; warns about unmapped roles", async ({
    page,
    api,
  }) => {
    const state = book(api)
    api.on("PUT", "/workspace/:ws/accounting/setup", () => {
      state.setup = setup({
        version: 4,
        account_mappings: { cash: "a-cash", revenue: "a-sales" },
        missing_roles: ["receivables"],
      })
      return { data: state.setup }
    })
    await page.goto("/accounting/setup")
    await expect(page.getByText("2 roles still need an account")).toBeVisible()
    await expect(page.getByText("Sales, Customer balances")).toBeVisible()
    await expect(page.getByLabel("Book currency")).toHaveText("TZS")
    await expect(page.getByLabel("Timezone")).toHaveValue(
      "Africa/Dar_es_Salaam"
    )
    await expect(page.getByLabel("Cash", { exact: true })).toHaveText(
      "1000 Cash in tills"
    )
    await expectFits(page)
    await expectNoRawValues(page)

    await pick(page.getByLabel("Sales", { exact: true }), "4000 Honey sales")
    await page.getByRole("button", { name: "Save setup" }).click()
    await expect(
      page.getByRole("status").filter({ hasText: "Setup saved." })
    ).toBeVisible()
    await expect(page.getByText("1 role still needs an account")).toBeVisible()
    const sent = api.callsTo("PUT", "/workspace/:ws/accounting/setup").at(-1)!
    expect(sent.body).toMatchObject({
      expected_version: 3,
      payload: {
        functional_currency_id: "cur-tzs",
        timezone: "Africa/Dar_es_Salaam",
        account_mappings: { cash: "a-cash", revenue: "a-sales" },
      },
    })
  })

  test("an active book's setup is read-only", async ({ page, api }) => {
    book(api, {
      setup: setup({
        state: "active",
        activated_at: "2026-11-01T06:00:00Z",
        missing_roles: [],
        configured: true,
        allowed_actions: [],
      }),
    })
    await page.goto("/accounting/setup")
    await expect(page.getByText(/The book has been active since/)).toBeVisible()
    await expect(page.getByLabel("Book currency")).toBeDisabled()
    await expect(page.getByRole("button", { name: "Save setup" })).toHaveCount(
      0
    )
    await expectFits(page)
    await expectNoRawValues(page)
  })
})

test.describe("journals", () => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/accounting/journals", status: 422 },
        {
          url: "/v1/workspace/w1/accounting/journals/j-1/post/review",
          status: 422,
        },
        { url: "/v1/workspace/w1/accounting/journals/j-1/post", status: 409 },
      ],
    },
  })

  test("a draft is created with its lines as typed, shown with server totals", async ({
    page,
    api,
  }) => {
    const state = book(api, { journals: [] })
    let tries = 0
    api.on("POST", "/workspace/:ws/accounting/journals", () => {
      tries += 1
      if (tries === 1)
        return {
          status: 422,
          body: {
            message: "The given data was invalid.",
            errors: {
              "payload.lines.0.account_id": [
                "Control accounts move only through their own workflow.",
              ],
            },
          },
        }
      state.journals = [journal()]
      return { status: 201, body: { data: state.journals[0] } }
    })
    await page.goto("/accounting/journals")
    await expect(page.getByText("Accounting is not active yet.")).toBeVisible()
    await expect(page.getByText("No journals yet.")).toBeVisible()
    await page.getByRole("link", { name: "New journal" }).click()
    await expect(page.getByLabel("Currency")).toHaveText("TZS")
    await page.getByLabel("Accounting date").fill("2026-11-15")
    await page.getByLabel("Reason").fill("Accrue November rent")
    const accounts = page.getByLabel("Account", { exact: true })
    await pick(accounts.nth(0), "1000 Cash in tills")
    await page.getByLabel("Debit").nth(0).fill("150000")
    await pick(accounts.nth(1), "2400 Accrued costs")
    await page.getByLabel("Credit").nth(1).fill("150000")
    await expectFits(page)
    await page.getByRole("button", { name: "Save draft" }).click()
    await expect(
      page.getByText("Control accounts move only through their own workflow.")
    ).toBeVisible()

    await pick(accounts.nth(0), "6100 Rent")
    await page.getByRole("button", { name: "Save draft" }).click()
    await expect(page).toHaveURL(/\/accounting\/journals\/j-1/)
    await expect(page.getByText("Draft journal saved.")).toBeVisible()
    await expect(page.getByText("TZS 150,000.00").first()).toBeVisible()
    await expectNoRawValues(page)
    const sent = api
      .callsTo("POST", "/workspace/:ws/accounting/journals")
      .at(-1)!
    expect(sent.body).toMatchObject({
      payload: {
        accounting_date: "2026-11-15",
        currency_id: "cur-tzs",
        reason: "Accrue November rent",
        lines: [
          {
            account_id: "a-rent",
            debit: "150000",
            credit: "0",
            source_ref: null,
          },
          {
            account_id: "a-accr",
            debit: "0",
            credit: "150000",
            source_ref: null,
          },
        ],
      },
    })
  })

  test("posting before the book is active says so in words", async ({
    page,
    api,
  }) => {
    book(api)
    api.on(
      "POST",
      "/workspace/:ws/accounting/journals/:id/post/review",
      () => ({
        status: 422,
        body: {
          message: "accounting_not_configured",
          code: "accounting_not_configured",
        },
      })
    )
    await page.goto("/accounting/journals/j-1")
    await page
      .getByRole("region", { name: "Next step" })
      .getByRole("button", { name: "Post" })
      .click()
    const dialog = page.getByRole("dialog")
    await expect(
      dialog.getByText(/Accounting is not active yet\. Posting starts/)
    ).toBeVisible()
    await expect(dialog.getByRole("button", { name: "Post" })).toBeDisabled()
    await expectNoRawValues(page)
  })

  test("a draft posts through the server's review and echoes its fingerprint", async ({
    page,
    api,
  }) => {
    const state = book(api, {
      setup: setup({ state: "active", allowed_actions: [] }),
    })
    api.on(
      "POST",
      "/workspace/:ws/accounting/journals/:id/post/review",
      () => ({
        data: {
          action: "post",
          journal_id: "j-1",
          version: 1,
          accounting_date: "2026-11-15",
          period_id: "p-11",
          currency: TZS,
          functional_currency: TZS,
          lines: posted().lines,
          totals: posted().totals,
          review_fingerprint: "fp-post",
        },
      })
    )
    api.on("POST", "/workspace/:ws/accounting/journals/:id/post", () => {
      state.journals = [posted()]
      return { data: state.journals[0] }
    })
    await page.goto("/accounting/journals/j-1")
    const next = page.getByRole("region", { name: "Next step" })
    await expect(next.getByRole("link", { name: "Edit draft" })).toBeVisible()
    await expectFits(page)
    await next.getByRole("button", { name: "Post" }).click()
    const dialog = page.getByRole("dialog")
    await expect(dialog).toContainText("Debit TZS 150,000.00")
    await expect(dialog).toContainText("Credit TZS 150,000.00")
    await dialog.getByRole("button", { name: "Post" }).click()
    await expect(
      page.getByRole("status").filter({ hasText: "Journal 12 posted." })
    ).toBeVisible()
    await expect(
      page.getByRole("heading", { name: "Journal 12" })
    ).toBeVisible()
    await expect(next.getByRole("link", { name: "Edit draft" })).toHaveCount(0)
    await expectNoRawValues(page)
    expect(
      api.callsTo("POST", "/workspace/:ws/accounting/journals/:id/post").at(-1)!
        .body
    ).toMatchObject({
      expected_version: 1,
      payload: { review_fingerprint: "fp-post" },
    })
  })

  test("a stale post reloads the journal and says so", async ({
    page,
    api,
  }) => {
    const state = book(api, { setup: setup({ state: "active" }) })
    api.on(
      "POST",
      "/workspace/:ws/accounting/journals/:id/post/review",
      () => ({
        data: {
          action: "post",
          journal_id: "j-1",
          version: 1,
          accounting_date: "2026-11-15",
          period_id: "p-11",
          currency: TZS,
          functional_currency: TZS,
          lines: posted().lines,
          totals: posted().totals,
          review_fingerprint: "fp-old",
        },
      })
    )
    api.on("POST", "/workspace/:ws/accounting/journals/:id/post", () => {
      state.journals = [
        journal({ version: 2, reason: "Accrue November rent (fixed)" }),
      ]
      return {
        status: 409,
        body: { message: "stale_document", code: "stale_document" },
      }
    })
    await page.goto("/accounting/journals/j-1")
    await page
      .getByRole("region", { name: "Next step" })
      .getByRole("button", { name: "Post" })
      .click()
    await page.getByRole("dialog").getByRole("button", { name: "Post" }).click()
    await expect(
      page.getByRole("main").getByText("This changed since you opened it.")
    ).toBeVisible()
    await expect(
      page.getByText("Accrue November rent (fixed)").first()
    ).toBeVisible()
  })

  test("a posted manual journal reverses through its review; the reversal links back", async ({
    page,
    api,
  }) => {
    const reversal = posted({
      id: "j-2",
      number: 13,
      kind: "reversal",
      reversal_of: "j-1",
      accounting_date: "2026-11-20",
      reason: "Booked twice",
      allowed_actions: [],
    })
    const state = book(api, {
      setup: setup({ state: "active" }),
      journals: [posted()],
    })
    api.on(
      "POST",
      "/workspace/:ws/accounting/journals/:id/reverse/review",
      () => ({
        data: {
          action: "reverse",
          journal_id: "j-1",
          version: 2,
          accounting_date: "2026-11-20",
          period_id: "p-11",
          currency: TZS,
          functional_currency: TZS,
          lines: [
            line(1, "a-rent", "6100", "0.00", "150000.00"),
            line(2, "a-accr", "2400", "150000.00", "0.00"),
          ],
          totals: posted().totals,
          review_fingerprint: "fp-rev",
        },
      })
    )
    api.on("POST", "/workspace/:ws/accounting/journals/:id/reverse", () => {
      state.journals = [
        posted({ reversed_by: "j-2", allowed_actions: [] }),
        reversal,
      ]
      return { status: 201, body: { data: reversal } }
    })
    await page.goto("/accounting/journals/j-1")
    const next = page.getByRole("region", { name: "Next step" })
    await next.getByRole("button", { name: "Reverse" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Accounting date").fill("2026-11-20")
    await dialog.getByLabel("Reason").fill("Booked twice")
    await dialog.getByRole("button", { name: "Check reversal" }).click()
    await expect(dialog).toContainText("Credit TZS 150,000.00")
    await expectFits(page)
    await dialog.getByRole("button", { name: "Reverse" }).click()
    await expect(
      page
        .getByRole("status")
        .filter({ hasText: "Journal 13 booked as the reversal." })
    ).toBeVisible()
    await expect(
      page.getByText("This journal is posted and can't be changed.")
    ).toBeVisible()
    await page.getByRole("link", { name: "Open the reversal" }).click()
    await expect(
      page.getByRole("heading", { name: "Journal 13" })
    ).toBeVisible()
    await expect(
      page.getByRole("link", { name: "Open the journal this reverses" })
    ).toBeVisible()
    await expectNoRawValues(page)
    expect(
      api
        .callsTo("POST", "/workspace/:ws/accounting/journals/:id/reverse")
        .at(-1)!.body
    ).toMatchObject({
      expected_version: 2,
      payload: {
        accounting_date: "2026-11-20",
        reason: "Booked twice",
        review_fingerprint: "fp-rev",
      },
    })
  })
})

test.describe("reports", () => {
  const tb = {
    from: "2026-11-01",
    to: "2026-12-01",
    functional_currency: TZS,
    rows: [
      {
        account: { id: "a-rent", code: "6100", name: "Rent", type: "expense" },
        opening_debit: "0.00",
        opening_credit: "0.00",
        debit: "150000.00",
        credit: "0.00",
        closing_debit: "150000.00",
        closing_credit: "0.00",
      },
      {
        account: {
          id: "a-accr",
          code: "2400",
          name: "Accrued costs",
          type: "liability",
        },
        opening_debit: "0.00",
        opening_credit: "0.00",
        debit: "0.00",
        credit: "150000.00",
        closing_debit: "0.00",
        closing_credit: "150000.00",
      },
    ],
    totals: {
      opening_debit: "0.00",
      opening_credit: "0.00",
      debit: "150000.00",
      credit: "150000.00",
      closing_debit: "150000.00",
      closing_credit: "150000.00",
    },
  }
  const ledgerLine = (n: number) => ({
    entry_id: `j-${n}`,
    number: n,
    kind: "manual",
    accounting_date: "2026-11-15",
    reason: `Rent part ${n}`,
    line_no: 1,
    relation_id: null,
    source_ref: null,
    debit: "1000.00",
    credit: "0.00",
    balance: `${n * 1000}.00`,
  })

  test.use({
    allow: {
      responses: [
        {
          url: "/v1/workspace/w1/accounting/reports/trial-balance?from=2026-12-01&to=2026-11-01",
          status: 422,
        },
      ],
    },
  })

  test("trial balance, ledger with pages, profit and loss and balance sheet show the API's figures", async ({
    page,
    api,
  }) => {
    book(api, { setup: setup({ state: "active" }) })
    api.on(
      "GET",
      "/workspace/:ws/accounting/reports/trial-balance",
      ({ query }) =>
        query.get("from") === "2026-12-01"
          ? {
              status: 422,
              body: {
                message: "The given data was invalid.",
                errors: { to: ["Must be after from."] },
              },
            }
          : { data: tb }
    )
    api.on(
      "GET",
      "/workspace/:ws/accounting/reports/general-ledger",
      ({ query }) => {
        const p = Number(query.get("page") ?? 1)
        const all = Array.from({ length: 60 }, (_, i) => ledgerLine(i + 1))
        return {
          data: {
            account: {
              id: "a-rent",
              code: "6100",
              name: "Rent",
              type: "expense",
            },
            from: "2026-11-01",
            to: "2026-12-01",
            functional_currency: TZS,
            opening_balance: "0.00",
            debit: "60000.00",
            credit: "0.00",
            closing_balance: "60000.00",
            lines: paged(all, p, 50),
          },
        }
      }
    )
    api.on("GET", "/workspace/:ws/accounting/reports/profit-and-loss", () => ({
      data: {
        from: "2026-11-01",
        to: "2026-12-01",
        functional_currency: TZS,
        income: [
          {
            account: {
              id: "a-sales",
              code: "4000",
              name: "Honey sales",
              type: "income",
            },
            amount: "400000.00",
          },
        ],
        expenses: [
          {
            account: {
              id: "a-rent",
              code: "6100",
              name: "Rent",
              type: "expense",
            },
            amount: "150000.00",
          },
        ],
        total_income: "400000.00",
        total_expenses: "150000.00",
        net_profit: "250000.00",
      },
    }))
    api.on("GET", "/workspace/:ws/accounting/reports/balance-sheet", () => ({
      data: {
        as_of: "2026-11-30",
        functional_currency: TZS,
        assets: [
          {
            account: {
              id: "a-cash",
              code: "1000",
              name: "Cash in tills",
              type: "asset",
            },
            amount: "400000.00",
          },
        ],
        liabilities: [
          {
            account: {
              id: "a-accr",
              code: "2400",
              name: "Accrued costs",
              type: "liability",
            },
            amount: "150000.00",
          },
        ],
        equity: [],
        total_assets: "400000.00",
        total_liabilities: "150000.00",
        total_equity: "0.00",
        current_earnings: "250000.00",
        total_liabilities_and_equity: "400000.00",
      },
    }))

    await page.goto(
      "/accounting/reports?report=trial-balance&from=2026-11-01&to=2026-12-01"
    )
    await expect(page.getByRole("row", { name: /6100 Rent/ })).toContainText(
      "TZS 150,000.00"
    )
    await expect(page.getByRole("row", { name: /Totals/ })).toContainText(
      "TZS 150,000.00"
    )
    await expectFits(page)
    await expectNoRawValues(page)

    await page.getByLabel("From").fill("2026-12-01")
    await page.getByLabel("Up to (not including)").fill("2026-11-01")
    await page.getByRole("button", { name: "Show" }).click()
    await expect(page.getByText("Must be after from.")).toBeVisible()

    await page.getByLabel("Report").selectOption("general-ledger")
    await page.getByLabel("From").fill("2026-11-01")
    await page.getByLabel("Up to (not including)").fill("2026-12-01")
    await pick(page.getByLabel("Account", { exact: true }), "6100 Rent")
    await page.getByRole("button", { name: "Show" }).click()
    await expect(page.getByText("Closing balance")).toBeVisible()
    await expect(
      page.getByRole("link", { name: "Journal 50", exact: true })
    ).toBeVisible()
    await page.getByRole("button", { name: /Next/ }).click()
    await expect(
      page.getByRole("link", { name: "Journal 60", exact: true })
    ).toBeVisible()
    await expect(page).toHaveURL(/page=2/)
    await expectFits(page)
    await expectNoRawValues(page)

    await page.getByLabel("Report").selectOption("profit-and-loss")
    await page.getByRole("button", { name: "Show" }).click()
    await expect(page.getByRole("row", { name: /Net profit/ })).toContainText(
      "TZS 250,000.00"
    )

    await page.getByLabel("Report").selectOption("balance-sheet")
    await page.getByLabel("On", { exact: true }).fill("2026-11-30")
    await page.getByRole("button", { name: "Show" }).click()
    await expect(
      page.getByRole("row", { name: /Total liabilities and equity/ })
    ).toContainText("TZS 400,000.00")
    await expect(
      page.getByRole("row", { name: /Profit not yet closed/ })
    ).toContainText("TZS 250,000.00")
    await expectFits(page)
    await expectNoRawValues(page)
  })

  test("an empty range says so", async ({ page, api }) => {
    book(api)
    api.on("GET", "/workspace/:ws/accounting/reports/trial-balance", () => ({
      data: { ...tb, rows: [] },
    }))
    await page.goto("/accounting/reports")
    await expect(
      page.getByText("No posted entries in this range.")
    ).toBeVisible()
    await expectFits(page)
  })
})

test.describe("cash", () => {
  const drawer = {
    kind: "drawer",
    id: "reg-1",
    name: "Front counter",
    currency: TZS,
    balance: 5000000,
    open_shift_id: "sh-1",
  }
  test.use({
    allow: {
      responses: [
        {
          url: "/v1/workspace/w1/accounting/cash-adjustments/review",
          status: 422,
        },
      ],
    },
  })

  test("a count that differs posts through the review; equal counts are refused in words", async ({
    page,
    api,
  }) => {
    const state = {
      places: [
        drawer,
        {
          kind: "vault",
          id: "v-1",
          name: "Back safe",
          currency: TZS,
          balance: 0,
          open_shift_id: null,
        },
      ],
    }
    api.on("GET", "/workspace/:ws/accounting/cash-places", () => ({
      data: state.places,
    }))
    let reviews = 0
    api.on("POST", "/workspace/:ws/accounting/cash-adjustments/review", () => {
      reviews += 1
      if (reviews === 1)
        return {
          status: 422,
          body: {
            message: "The given data was invalid.",
            errors: {
              counted_amount: [
                "Nothing to adjust: the count matches the book.",
              ],
            },
          },
        }
      return {
        data: {
          cash_place: { kind: "drawer", id: "reg-1" },
          currency: TZS,
          expected: 5000000,
          counted: 4950000,
          difference: -50000,
          accounting_date: "2026-11-17",
          book: "active",
          journal: {
            debit_role: "cash_over_short",
            credit_role: "cash",
            amount: 50000,
          },
          review_fingerprint: "fp-cash",
        },
      }
    })
    api.on("POST", "/workspace/:ws/accounting/cash-adjustments", () => {
      state.places = [{ ...drawer, balance: 4950000 }, state.places[1]!]
      return {
        status: 201,
        body: {
          data: {
            id: "adj-1",
            cash_place: { kind: "drawer", id: "reg-1" },
            currency: TZS,
            expected: 5000000,
            counted: 4950000,
            difference: -50000,
            accounting_date: "2026-11-17",
            count_evidence_ref: "Sheet 17",
            reason: "Evening count",
            actor: { kind: "member", user_id: "u1" },
            till_user_id: null,
            journal_entry_id: "j-9",
            created_at: "2026-11-17T18:00:00Z",
          },
        },
      }
    })
    await page.goto("/accounting/cash")
    const row = page.getByRole("row", { name: /Front counter/ })
    await expect(row).toContainText("TZS 50,000.00")
    await expect(row).toContainText("Shift open")
    await expect(page.getByRole("row", { name: /Back safe/ })).toContainText(
      "Safe"
    )
    await expectFits(page)
    await expectNoRawValues(page)

    await row.getByRole("button", { name: "Count cash" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Counted (TZS)").fill("50000")
    await dialog.getByLabel("Accounting date").fill("2026-11-17")
    await dialog.getByRole("button", { name: "Compare with the book" }).click()
    await expect(
      dialog.getByText("Nothing to adjust: the count matches the book.")
    ).toBeVisible()

    await dialog.getByLabel("Counted (TZS)").fill("49500")
    await dialog.getByRole("button", { name: "Compare with the book" }).click()
    await expect(dialog).toContainText("-TZS 500.00")
    await expect(dialog).toContainText("debit Cash over and short, credit Cash")
    await dialog.getByLabel("Count sheet or evidence").fill("Sheet 17")
    await dialog.getByLabel("Reason").fill("Evening count")
    await expectFits(page)
    await dialog.getByRole("button", { name: "Record adjustment" }).click()
    await expect(
      page
        .getByRole("status")
        .filter({ hasText: "Cash at Front counter adjusted by -TZS 500.00." })
    ).toBeVisible()
    await expect(row).toContainText("TZS 49,500.00")
    const sent = api
      .callsTo("POST", "/workspace/:ws/accounting/cash-adjustments")
      .at(-1)!
    expect(sent.body).toMatchObject({
      payload: {
        cash_place: { kind: "drawer", id: "reg-1" },
        counted_amount: "49500",
        accounting_date: "2026-11-17",
        count_evidence_ref: "Sheet 17",
        reason: "Evening count",
        review_fingerprint: "fp-cash",
      },
    })
  })

  test("without the adjust right there is no count button", async ({
    page,
    api,
  }) => {
    api.on("GET", "/me", () => ({
      data: {
        user: {
          ...me,
          permissions: [{ workspace: "w1", permissions: ["accounting.read"] }],
        },
      },
    }))
    api.on("GET", "/workspace/:ws/accounting/cash-places", () => ({
      data: [drawer],
    }))
    await page.goto("/accounting/cash")
    await expect(page.getByRole("row", { name: /Front counter/ })).toBeVisible()
    await expect(page.getByRole("button", { name: "Count cash" })).toHaveCount(
      0
    )
  })

  test("no cash places yet says what to do", async ({ page, api }) => {
    api.on("GET", "/workspace/:ws/accounting/cash-places", () => ({ data: [] }))
    await page.goto("/accounting/cash")
    await expect(page.getByText("No cash places yet.")).toBeVisible()
    await expectFits(page)
  })
})

test.describe("opening balances", () => {
  const baseline = (n: number) => ({
    id: `b-${n}`,
    source_kind: "invoice",
    source_id: `inv-${n}`,
    number: String(1000 + n),
    party: `Customer ${n}`,
    currency: "TZS",
    face: "100.00",
    settled: "40.00",
    outstanding: "60.00",
    corrected_outstanding: "60.00",
    corrections: 0,
  })

  test("a long opening pages through the API's pages", async ({ page, api }) => {
    book(api, {
      setup: setup({
        state: "active",
        activated_at: "2026-11-01T06:00:00Z",
        missing_roles: [],
        configured: true,
      }),
    })
    const all = Array.from({ length: 60 }, (_, i) => baseline(i + 1))
    const asked: (string | null)[] = []
    api.on("GET", "/workspace/:ws/accounting/opening-baselines", ({ query }) => {
      asked.push(query.get("page"))
      return paged(all, Number(query.get("page") ?? 1), 50)
    })
    await page.goto("/accounting/opening")
    await expect(page.getByText("Invoice 1001")).toBeVisible()
    await expect(page.getByText("1–50 of 60")).toBeVisible()
    await expect(page.getByText("Invoice 1051")).toHaveCount(0)
    await page.getByRole("button", { name: "Next" }).click()
    await expect(page.getByText("Invoice 1051")).toBeVisible()
    await expect(page.getByText("51–60 of 60")).toBeVisible()
    await expect(page.getByRole("button", { name: "Next" })).toBeDisabled()
    expect(asked).toEqual(["1", "2"])
    await expectFits(page)
    await expectNoRawValues(page)
  })

  test("one page shows no pager", async ({ page, api }) => {
    book(api, {
      setup: setup({
        state: "active",
        activated_at: "2026-11-01T06:00:00Z",
        missing_roles: [],
        configured: true,
      }),
    })
    api.on("GET", "/workspace/:ws/accounting/opening-baselines", () =>
      paged([baseline(1)])
    )
    await page.goto("/accounting/opening")
    await expect(page.getByText("Invoice 1001")).toBeVisible()
    await expect(page.getByRole("button", { name: "Next" })).toHaveCount(0)
    await expectFits(page)
  })
})
