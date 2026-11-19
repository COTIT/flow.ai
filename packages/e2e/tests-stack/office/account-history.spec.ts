import {
  activeBook,
  addAccount,
  command,
  freshWorkspace,
} from "../../src/accounting"
import {
  apiAs,
  expect,
  loginOffice,
  stackAllow,
  test,
  urls,
} from "../../src/stack"
import { expectNoRawValues, pick } from "../../src/test"

const today = () => new Date().toISOString().slice(0, 10)
const missing = "01900000-0000-7000-8000-00000000abcd"
const pad = (n: number) => String(n).padStart(2, "0")
/** The month of `day` (YYYY-MM-DD) as the pages default to it (accounting/shared.tsx monthOf). */
const monthOf = (day: string) => {
  const [y = 0, m = 1] = day.split("-").map(Number)
  return {
    from: `${y}-${pad(m)}-01`,
    to: m === 12 ? `${y + 1}-01-01` : `${y}-${pad(m + 1)}-01`,
  }
}

test.describe("account history", () => {
  let w: Awaited<ReturnType<typeof freshWorkspace>>
  test.beforeAll(async ({ request }) => {
    w = await freshWorkspace(request, "acc-history")
  })
  test.use({
    // eslint-disable-next-line no-empty-pattern -- Playwright reads fixture deps from the pattern
    allow: async ({}, use) =>
      use({
        ...stackAllow,
        // The unknown account is the case under test; the page says it in words.
        responses: [
          ...stackAllow.responses,
          {
            url: `${urls.office}/v1/workspace/${w.api.workspace}/accounting/accounts/${missing}`,
            status: 404,
          },
        ],
      }),
  })

  test("an account opens from the chart with its ledger history, paged and filtered", async ({
    page,
    request,
  }) => {
    const api = await apiAs(request, w.login)
    const { currency, mappings } = await activeBook(request, api)
    const till = await addAccount(request, api, "1000", "Till cash", "asset")
    // One posted journal: 51 debit lines on Till cash, so the history has two pages of 50.
    const draft = await command<{ id: string; version: number }>(
      request,
      api,
      "POST",
      "/accounting/journals",
      {
        payload: {
          accounting_date: today(),
          currency_id: currency.id,
          reason: "Daily takings",
          lines: [
            ...Array.from({ length: 51 }, () => ({
              account_id: till.id,
              debit: "1",
              credit: "0",
            })),
            { account_id: mappings.revenue, debit: "0", credit: "51" },
          ],
        },
      }
    )
    const review = await api.post<{ review_fingerprint: string }>(
      api.ws(`/accounting/journals/${draft.id}/post/review`)
    )
    await command(
      request,
      api,
      "POST",
      `/accounting/journals/${draft.id}/post`,
      {
        expected_version: draft.version,
        payload: { review_fingerprint: review.review_fingerprint },
      }
    )
    await loginOffice(page, w.login)

    await page.goto(`${urls.office}/accounting/accounts`)
    await page.getByRole("link", { name: "1000", exact: true }).click()
    await expect(
      page.getByRole("heading", { name: "1000 Till cash" })
    ).toBeVisible()
    await expect(page.getByRole("cell", { name: /Daily takings/ })).toHaveCount(
      50
    )
    await expectNoRawValues(page)
    // The default range is the book's month (business_date from the API, not the browser).
    const setup = await api.get<{ business_date: string }>(
      api.ws("/accounting/setup")
    )
    await expect(page.getByLabel("From")).toHaveValue(
      monthOf(setup.business_date).from
    )

    await page.getByRole("button", { name: /next/i }).click()
    await expect(page.getByRole("cell", { name: /Daily takings/ })).toHaveCount(
      1
    )

    // A range without postings.
    const next = new Date()
    next.setUTCFullYear(next.getUTCFullYear() + 1)
    const later = next.toISOString().slice(0, 10)
    await page.getByLabel("From").fill(later)
    await page
      .getByLabel("Up to (not including)")
      .fill(`${next.getUTCFullYear() + 1}-01-01`)
    await page.getByRole("button", { name: "Show" }).click()
    await expect(
      page.getByText("No posted entries in this range.")
    ).toBeVisible()
    await expectNoRawValues(page)
  })

  test("an unknown account shows a readable error", async ({ page }) => {
    await loginOffice(page, w.login)
    await page.goto(`${urls.office}/accounting/accounts/${missing}`)
    await expect(page.getByText("Not found")).toBeVisible()
    await expect(
      page.getByRole("link", { name: "Back to the list" })
    ).toBeVisible()
    await expectNoRawValues(page)
  })

  test("the ledger report finds an account by search, beyond the first page", async ({
    page,
    request,
  }) => {
    const own = await freshWorkspace(request, "acc-search")
    const api = await apiAs(request, own.login)
    // 27 accounts (26 roles + this one): more than one picker page of 20.
    await activeBook(request, api, { activate: false })
    await addAccount(request, api, "Z900", "Zebra suspense", "expense")
    await loginOffice(page, own.login)
    await page.goto(`${urls.office}/accounting/reports?report=general-ledger`)
    const picker = page.getByLabel("Account", { exact: true })
    await expect(picker).toHaveText("Choose an account")
    await pick(picker, "Z900 Zebra suspense", "zebra")
    await expect(picker).toHaveText("Z900 Zebra suspense")
    await page.getByRole("button", { name: "Show" }).click()
    await expect(page).toHaveURL(/account=/)
    await picker.click()
    await page
      .getByRole("searchbox", { name: "Search", exact: true })
      .fill("no-such-account")
    await expect(page.getByText("Nothing matches.")).toBeVisible()
    await expectNoRawValues(page)
  })
})
