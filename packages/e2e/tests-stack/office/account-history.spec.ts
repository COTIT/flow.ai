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
import { expectNoRawValues } from "../../src/test"

const today = () => new Date().toISOString().slice(0, 10)
const missing = "01900000-0000-7000-8000-00000000abcd"
const pad = (n: number) => String(n).padStart(2, "0")
/** The page's default range: this month in browser time (accounting/shared.tsx thisMonth). */
const month = () => {
  const d = new Date()
  const n = new Date(d.getFullYear(), d.getMonth() + 1, 1)
  return {
    from: `${d.getFullYear()}-${pad(d.getMonth() + 1)}-01`,
    to: `${n.getFullYear()}-${pad(n.getMonth() + 1)}-01`,
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
            url: `${urls.office}/v1/workspace/${w.api.workspace}/accounting/reports/general-ledger?account_id=${missing}&from=${month().from}&to=${month().to}`,
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
    await expect(
      page.getByRole("heading", { name: "Account history" })
    ).toBeVisible()
    // One readable error; the other role=alert is Next's empty #__next-route-announcer__.
    await expect(page.getByRole("alert").filter({ hasText: /\S/ })).toBeVisible()
    await expectNoRawValues(page)
  })
})
