import type { Page } from "@playwright/test"

import {
  activeBook,
  addAccount,
  command,
  freshWorkspace,
  mainCurrency,
} from "../../src/accounting"
import {
  apiAs,
  cred,
  expect,
  loginOffice,
  stackAllow,
  test,
  urls,
} from "../../src/stack"
import { expectNoRawValues, pick } from "../../src/test"

const today = () => new Date().toISOString().slice(0, 10)

/** The detail page's "Next step" card. */
const next = (page: Page) => page.getByRole("region", { name: "Next step" })

test.describe("a book being set up", () => {
  let w: Awaited<ReturnType<typeof freshWorkspace>>
  let cashAccount: string
  let refusedDraft: string
  test.beforeAll(async ({ request }) => {
    w = await freshWorkspace(request, "acc-setup")
    cashAccount = (
      await addAccount(request, w.api, "1000", "Cash in tills", "asset")
    ).id
    // A draft made through the API, so the refused post has a known address.
    const currency = await mainCurrency(w.api)
    const other = await addAccount(
      request,
      w.api,
      "9000",
      "Suspense",
      "liability"
    )
    refusedDraft = (
      await command<{ id: string }>(
        request,
        w.api,
        "POST",
        "/accounting/journals",
        {
          payload: {
            accounting_date: today(),
            currency_id: currency.id,
            reason: "Waiting for activation",
            lines: [
              { account_id: other.id, debit: "1", credit: "0" },
              { account_id: cashAccount, debit: "0", credit: "1" },
            ],
          },
        }
      )
    ).id
  })
  test.use({
    // eslint-disable-next-line no-empty-pattern -- Playwright reads fixture deps from the pattern
    allow: async ({}, use) => {
      const ws = `${urls.office}/v1/workspace/${w.api.workspace}`
      await use({
        ...stackAllow,
        // Each refusal is the case under test; the page shows it in words.
        responses: [
          ...stackAllow.responses,
          { url: `${ws}/accounting/accounts/${cashAccount}`, status: 422 },
          { url: `${ws}/accounting/periods`, status: 422 },
          {
            url: `${ws}/accounting/journals/${refusedDraft}/post/review`,
            status: 422,
          },
        ],
      })
    },
  })

  test("setup, chart, periods and a draft journal; posting waits for activation", async ({
    page,
    request,
  }) => {
    const { login } = w
    const api = await apiAs(request, login)
    const currency = await mainCurrency(api)
    await loginOffice(page, login)

    // Chart: add three accounts in the screen.
    await page.goto(`${urls.office}/accounting/accounts`)
    await expect(page.getByText("2 accounts")).toBeVisible()
    for (const [code, name, type] of [
      ["2400", "Accrued costs", "liability"],
      ["6100", "Rent", "expense"],
    ] as const) {
      await page.getByRole("button", { name: "Add account" }).first().click()
      const dialog = page.getByRole("dialog")
      await dialog.getByLabel("Code").fill(code)
      await dialog.getByLabel("Name").fill(name)
      await dialog.getByLabel("Type").selectOption(type)
      await dialog.getByRole("button", { name: "Add account" }).click()
      await expect(
        page.getByRole("status").filter({ hasText: `Account ${code} added.` })
      ).toBeVisible()
    }
    await expect(page.getByText("4 accounts")).toBeVisible()
    await expectNoRawValues(page)

    // Setup: currency, timezone, the cash role.
    await page
      .getByRole("navigation", { name: "Accounting sections" })
      .getByRole("link", { name: "Setup" })
      .click()
    await expect(page.getByText("26 roles still need an account")).toBeVisible()
    await pick(
      page.getByLabel("Book currency"),
      new RegExp(`^${currency.asset_type}`, "i"),
      currency.asset_type
    )
    await page.getByLabel("Timezone").selectOption("Africa/Dar_es_Salaam")
    await pick(page.getByLabel("Cash", { exact: true }), "1000 Cash in tills")
    await page.getByRole("button", { name: "Save setup" }).click()
    await expect(
      page.getByRole("status").filter({ hasText: "Setup saved." })
    ).toBeVisible()
    await expect(page.getByText("25 roles still need an account")).toBeVisible()
    await expectNoRawValues(page)

    // A mapped account can't be switched off: said in words.
    await page.goto(`${urls.office}/accounting/accounts`)
    const cash = page.getByRole("row", { name: /1000/ })
    await expect(cash).toContainText("Cash")
    await cash.getByRole("button", { name: "Edit" }).click()
    await page
      .getByRole("dialog")
      .getByRole("checkbox", { name: "In use" })
      .click()
    await page.getByRole("dialog").getByRole("button", { name: "Save" }).click()
    await expect(
      page
        .getByRole("dialog")
        .locator(".text-destructive, [role=alert]")
        .first()
    ).toBeVisible()
    await page.keyboard.press("Escape")

    // Periods: a year, its month, and the same month again refused.
    const year = new Date().getUTCFullYear()
    const month = today().slice(0, 7)
    const nextMonth = new Date(Date.UTC(year, new Date().getUTCMonth() + 1, 1))
      .toISOString()
      .slice(0, 10)
    await page.goto(`${urls.office}/accounting/periods`)
    await page.getByRole("button", { name: "Add fiscal year" }).click()
    let dialog = page.getByRole("dialog")
    await dialog.getByLabel("Starts on").fill(`${year}-01-01`)
    await dialog.getByLabel("Ends before").fill(`${year + 1}-01-01`)
    await dialog.getByRole("button", { name: "Add year" }).click()
    await expect(
      page.getByRole("status").filter({ hasText: "Fiscal year added." })
    ).toBeVisible()
    for (const attempt of [1, 2]) {
      await page.getByRole("button", { name: "Add period" }).click()
      dialog = page.getByRole("dialog")
      await dialog.getByLabel("Starts on").fill(`${month}-01`)
      await dialog.getByLabel("Ends before").fill(nextMonth)
      await dialog.getByRole("button", { name: "Add period" }).click()
      if (attempt === 1) {
        await expect(
          page.getByRole("status").filter({ hasText: "Period added." })
        ).toBeVisible()
        await expect(page.getByRole("dialog")).toHaveCount(0)
      } else {
        await expect(
          dialog.locator(".text-destructive, [role=alert]").first()
        ).toBeVisible()
        await page.keyboard.press("Escape")
      }
    }
    await expect(page.getByRole("cell", { name: "Open" })).toHaveCount(1)
    await expectNoRawValues(page)

    // A draft journal; posting before activation is refused in words.
    await page.goto(`${urls.office}/accounting/journals`)
    await expect(page.getByText("Accounting is not active yet.")).toBeVisible()
    await page.getByRole("link", { name: "New journal" }).click()
    await page.getByLabel("Accounting date").fill(today())
    await page.getByLabel("Reason").fill("Accrue rent")
    const accounts = page.getByLabel("Account", { exact: true })
    await pick(accounts.nth(0), "6100 Rent")
    await page.getByLabel("Debit").nth(0).fill("1500")
    await pick(accounts.nth(1), "2400 Accrued costs")
    await page.getByLabel("Credit").nth(1).fill("1500")
    await page.getByRole("button", { name: "Save draft" }).click()
    await expect(page.getByText("Draft journal saved.")).toBeVisible()
    await expect(next(page).getByRole("button", { name: "Post" })).toBeVisible()
    await page.goto(`${urls.office}/accounting/journals/${refusedDraft}`)
    await next(page).getByRole("button", { name: "Post" }).click()
    await expect(
      page
        .getByRole("dialog")
        .getByText(/Accounting is not active yet\. Posting starts/)
    ).toBeVisible()
    await expectNoRawValues(page)
    await page.keyboard.press("Escape")

    // Reports of a book with nothing posted.
    await page.goto(`${urls.office}/accounting/reports`)
    await expect(
      page.getByText("No posted entries in this range.")
    ).toBeVisible()
  })
})

test("an active book posts, reverses and reports a manual journal; another account can't read it", async ({
  page,
  request,
}) => {
  const { login, api } = await freshWorkspace(request, "acc-active")
  await activeBook(request, api)
  await addAccount(request, api, "2400", "Accrued costs", "liability")
  await addAccount(request, api, "6100", "Rent", "expense")
  await loginOffice(page, login)

  await page.goto(`${urls.office}/accounting/journals/new`)
  await page.getByLabel("Accounting date").fill(today())
  await page.getByLabel("Reason").fill("Accrue rent")
  const accounts = page.getByLabel("Account", { exact: true })
  await pick(accounts.nth(0), "6100 Rent")
  await page.getByLabel("Debit").nth(0).fill("1500")
  await pick(accounts.nth(1), "2400 Accrued costs")
  await page.getByLabel("Credit").nth(1).fill("1500")
  await page.getByRole("button", { name: "Save draft" }).click()
  await expect(page.getByText("Draft journal saved.")).toBeVisible()
  const journalUrl = page.url().split("?")[0]!

  // Post through the review.
  await next(page).getByRole("button", { name: "Post" }).click()
  let dialog = page.getByRole("dialog")
  await expect(dialog).toContainText(/Debit [A-Z]{3} 1,500\.00/)
  await dialog.getByRole("button", { name: "Post" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: /Journal \d+ posted\./ })
  ).toBeVisible()
  await expect(
    page.getByRole("heading", { name: /^Journal \d+$/ })
  ).toBeVisible()
  await expectNoRawValues(page)

  // Reports carry the posted figures.
  await page.goto(`${urls.office}/accounting/reports`)
  await expect(page.getByRole("row", { name: /6100 Rent/ })).toContainText(
    "1,500.00"
  )
  await expect(page.getByRole("row", { name: /Totals/ })).toContainText(
    "1,500.00"
  )
  await page.getByLabel("Report").selectOption("profit-and-loss")
  await page.getByRole("button", { name: "Show" }).click()
  await expect(page.getByRole("row", { name: /Net profit/ })).toContainText("-")
  await page.getByLabel("Report").selectOption("general-ledger")
  await pick(page.getByLabel("Account", { exact: true }), "6100 Rent")
  await page.getByRole("button", { name: "Show" }).click()
  await expect(page.getByRole("link", { name: /^Journal \d+$/ })).toBeVisible()
  await page.getByLabel("Report").selectOption("balance-sheet")
  await page.getByRole("button", { name: "Show" }).click()
  await expect(
    page.getByRole("row", { name: /Total liabilities and equity/ })
  ).toBeVisible()
  await expectNoRawValues(page)

  // Reverse through its review; the reversal links back.
  await page.goto(journalUrl)
  await next(page).getByRole("button", { name: "Reverse" }).click()
  dialog = page.getByRole("dialog")
  await dialog.getByLabel("Accounting date").fill(today())
  await dialog.getByLabel("Reason").fill("Booked twice")
  await dialog.getByRole("button", { name: "Check reversal" }).click()
  await dialog.getByRole("button", { name: "Reverse" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: /booked as the reversal/ })
  ).toBeVisible()
  await page.getByRole("link", { name: "Open the reversal" }).click()
  await expect(
    page.getByRole("link", { name: "Open the journal this reverses" })
  ).toBeVisible()
  await expectNoRawValues(page)

  // Another account's member is refused the journal.
  const other = await apiAs(request, cred("office (AFM)", "owner"))
  const id = journalUrl.split("/").at(-1)!
  const res = await request.get(
    `${urls.api}/v1/workspace/${api.workspace}/accounting/journals/${id}`,
    {
      headers: { Authorization: `Bearer ${other.token}` },
    }
  )
  expect([403, 404]).toContain(res.status())
})

test.describe("cash places", () => {
  let w: Awaited<ReturnType<typeof freshWorkspace>>
  test.beforeAll(async ({ request }) => {
    w = await freshWorkspace(request, "acc-cash")
  })
  test.use({
    // eslint-disable-next-line no-empty-pattern -- Playwright reads fixture deps from the pattern
    allow: async ({}, use) =>
      use({
        ...stackAllow,
        responses: [
          ...stackAllow.responses,
          {
            url: `${urls.office}/v1/workspace/${w.api.workspace}/accounting/cash-adjustments/review`,
            status: 422,
          },
        ],
      }),
  })

  test("a counted drawer is adjusted through the review; an equal count is refused", async ({
    page,
    request,
  }) => {
    const { login } = w
    const api = await apiAs(request, login)
    const { currency } = await activeBook(request, api)
    // A fresh workspace has no warehouse yet; a register needs a store.
    await api.post(api.ws("/warehouses"), {
      name: "Shop floor",
      is_store: true,
      default_warehouse: true,
    })
    const store = (
      await api.get<{ id: string; name: string }[]>(
        api.ws("/warehouses?page_size=100")
      )
    ).find((x) => x.name === "Shop floor")!
    await api.post(api.ws("/till/registers"), {
      name: "Front counter",
      warehouse_id: store.id,
      currency_id: currency.id,
    })
    await loginOffice(page, login)

    await page.goto(`${urls.office}/accounting/cash`)
    const row = page.getByRole("row", { name: /Front counter/ })
    await expect(row).toContainText("Till drawer")
    await expectNoRawValues(page)

    await row.getByRole("button", { name: "Count cash" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel(/^Counted/).fill("0")
    await dialog.getByLabel("Accounting date").fill(today())
    await dialog.getByRole("button", { name: "Compare with the book" }).click()
    await expect(
      dialog.locator(".text-destructive, [role=alert]").first()
    ).toBeVisible()

    await dialog.getByLabel(/^Counted/).fill("25")
    await dialog.getByRole("button", { name: "Compare with the book" }).click()
    await expect(dialog).toContainText("Difference")
    await expect(dialog).toContainText("debit Cash, credit Cash over and short")
    await dialog.getByLabel("Count sheet or evidence").fill("Sheet 1")
    await dialog.getByLabel("Reason").fill("Found in the drawer")
    await dialog.getByRole("button", { name: "Record adjustment" }).click()
    await expect(
      page
        .getByRole("status")
        .filter({ hasText: "Cash at Front counter adjusted by" })
    ).toBeVisible()
    await expect(row).toContainText("25.00")
    await expectNoRawValues(page)

    // The adjustment booked a source journal in the active book.
    await page.goto(`${urls.office}/accounting/journals`)
    await expect(page.getByText("From a business document")).toBeVisible()
  })
})

test.describe("tax profiles", () => {
  let w: Awaited<ReturnType<typeof freshWorkspace>>
  let overlapping: string
  test.beforeAll(async ({ request }) => {
    w = await freshWorkspace(request, "tax")
    // A second lineage for the same country and dates: its verify must be refused later.
    overlapping = (
      await command<{ id: string }>(request, w.api, "POST", "/tax-profiles", {
        payload: {
          country: "TZ",
          effective_from: "2026-07-01",
          transaction_types: ["sale"],
          product_categories: [],
          components: [
            {
              code: "VAT",
              name: "VAT",
              kind: "percentage",
              rate: "16",
              recoverable: true,
            },
          ],
          exemptions: [],
          tax_point: "invoice",
          evidence_ref: "Draft notice",
        },
      })
    ).id
  })
  test.use({
    // eslint-disable-next-line no-empty-pattern -- Playwright reads fixture deps from the pattern
    allow: async ({}, use) =>
      use({
        ...stackAllow,
        responses: [
          ...stackAllow.responses,
          {
            url: `${urls.office}/v1/workspace/${w.api.workspace}/tax-profiles/${overlapping}/verify`,
            status: 422,
          },
        ],
      }),
  })

  test("a profile is drafted, verified and corrected; an overlapping one is refused", async ({
    page,
  }) => {
    const { login } = w
    await loginOffice(page, login)

    await page.goto(`${urls.office}/settings/taxes`)
    await page.getByRole("link", { name: "New tax profile" }).click()
    await page.getByLabel("Country (two letters, e.g. TZ)").fill("TZ")
    await page.getByLabel("In force from").fill("2026-07-01")
    await page.getByLabel("Code", { exact: true }).fill("VAT")
    await page.getByLabel("Name", { exact: true }).fill("VAT")
    await page.getByLabel("Rate (%)").fill("18")
    await page
      .getByLabel("Evidence (law, notice or letter)")
      .fill("Finance Act 2026")
    await page.getByRole("button", { name: "Save draft" }).click()
    await expect(page.getByText("Tax profile saved as draft.")).toBeVisible()

    await next(page).getByRole("button", { name: "Verify" }).click()
    let dialog = page.getByRole("dialog")
    await expect(dialog).toContainText("18%")
    await dialog.getByRole("button", { name: "Verify" }).click()
    await expect(
      page.getByRole("status").filter({ hasText: "Tax profile verified." })
    ).toBeVisible()
    await expectNoRawValues(page)

    // The second lineage for the same country and dates can't be verified.
    const first = page.url().split("?")[0]!
    await page.goto(`${urls.office}/settings/taxes/${overlapping}`)
    await next(page).getByRole("button", { name: "Verify" }).click()
    dialog = page.getByRole("dialog")
    await dialog.getByRole("button", { name: "Verify" }).click()
    await expect(
      dialog.getByText("Another verified profile already covers")
    ).toBeVisible()
    await page.keyboard.press("Escape")

    // Correct the verified one: a new revision that replaces it once verified.
    await page.goto(first)
    await next(page).getByRole("link", { name: "Make a correction" }).click()
    await page.getByLabel("Rate (%)").fill("16")
    await page.getByRole("button", { name: "Save draft" }).click()
    await expect(
      page.getByRole("heading", { name: /revision 2/ })
    ).toBeVisible()
    await next(page).getByRole("button", { name: "Verify" }).click()
    await page
      .getByRole("dialog")
      .getByRole("button", { name: "Verify" })
      .click()
    await expect(
      page.getByRole("status").filter({ hasText: "Tax profile verified." })
    ).toBeVisible()
    await page
      .getByRole("link", { name: "Open the revision this corrects" })
      .click()
    await expect(
      page.getByText("A newer revision replaced this profile.")
    ).toBeVisible()
    await expectNoRawValues(page)
  })
})
