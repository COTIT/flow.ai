import {
  activeBook,
  addAccount,
  command,
  freshWorkspace,
  mainCurrency,
} from "../../src/accounting"
import { today } from "../../src/erp"
import { expect, loginOffice, stackAllow, test, urls } from "../../src/stack"
import { expectNoRawValues } from "../../src/test"

type Page = import("@playwright/test").Page

/** Choose a kind, fill its header with `before`, upload `csv`, map columns, check rows. */
async function importFile(
  page: Page,
  kind: string,
  name: string,
  csv: string,
  before?: () => Promise<void>
) {
  await page.goto(`${urls.office}/settings/imports/new`)
  await page.getByRole("combobox", { name: "What to import" }).click()
  await page.getByRole("option", { name: kind, exact: true }).click()
  if (before) await before()
  await page
    .getByLabel(/Spreadsheet/)
    .setInputFiles({ name, mimeType: "text/csv", buffer: Buffer.from(csv) })
  await expect(
    page.getByText(new RegExp(`rows found in ${name}`))
  ).toBeVisible()
  await page.getByRole("button", { name: "Check rows" }).click()
  await expect(page).toHaveURL(/\/settings\/imports\/[0-9a-f-]{36}$/)
}

test("a bank statement imports into its bank account", async ({
  page,
  request,
}) => {
  const w = await freshWorkspace(request, "imp-bank")
  await activeBook(request, w.api)
  const currency = await mainCurrency(w.api)
  const gl = await addAccount(request, w.api, "1100", "Main bank", "asset")
  await command(request, w.api, "POST", "/accounting/bank-accounts", {
    payload: { name: "Main bank", currency_id: currency.id, account_id: gl.id },
  })
  await loginOffice(page, w.login)
  const name = "statement.csv"
  await importFile(
    page,
    "Bank statement",
    name,
    `Booking date,Amount,Description\n${today()},40.00,Card payout\n${today()},-5.00,Fee\n`,
    async () => {
      await page.getByLabel("Bank account").selectOption({ label: "Main bank" })
      await page.getByLabel("Statement reference").fill("ST-1")
      await page.getByLabel("From", { exact: true }).fill(today())
      await page.getByLabel("To", { exact: true }).fill(today())
      await page.getByLabel("Opening balance").fill("0")
      await page.getByLabel("Closing balance").fill("35.00")
    }
  )
  await expect(
    page.getByText("2 ready · 0 need a fix · 0 imported")
  ).toBeVisible()
  await page.getByRole("button", { name: "Import 2 ready rows" }).click()
  await expect(page.getByText("Import finished: 2 rows imported.")).toBeVisible(
    {
      timeout: 30_000,
    }
  )
  // The rows page shows the file's own cells as typed.
  await expectNoRawValues(page, { allow: [name, today()] })
})

test.describe("a statement without its header", () => {
  let w: Awaited<ReturnType<typeof freshWorkspace>>
  test.beforeAll(async ({ request }) => {
    w = await freshWorkspace(request, "imp-bank-bad")
    await activeBook(request, w.api)
  })
  test.use({
    // eslint-disable-next-line no-empty-pattern -- Playwright reads fixture deps from the pattern
    allow: async ({}, use) =>
      use({
        ...stackAllow,
        // The refused header is the case under test; the page says it in words.
        responses: [
          ...stackAllow.responses,
          {
            url: `${urls.office}/v1/workspace/${w.api.workspace}/imports`,
            status: 422,
          },
        ],
      }),
  })

  test("is refused in words", async ({ page }) => {
    await loginOffice(page, w.login)
    await page.goto(`${urls.office}/settings/imports/new`)
    await page.getByRole("combobox", { name: "What to import" }).click()
    await page
      .getByRole("option", { name: "Bank statement", exact: true })
      .click()
    await page.getByLabel(/Spreadsheet/).setInputFiles({
      name: "bad.csv",
      mimeType: "text/csv",
      buffer: Buffer.from(`Booking date,Amount\n${today()},1.00\n`),
    })
    await page.getByRole("button", { name: "Check rows" }).click()
    await expect(
      page.getByText(
        "Choose the bank account and fill in every statement detail."
      )
    ).toBeVisible()
    await expect(page).toHaveURL(/\/settings\/imports\/new$/)
    await expectNoRawValues(page, { allow: ["bad.csv"] })
  })
})

test("opening trial balance: staged, then the opening review checks it (no activation)", async ({
  page,
  request,
}) => {
  const w = await freshWorkspace(request, "imp-opening")
  await activeBook(request, w.api, { activate: false })
  await addAccount(request, w.api, "3000", "Owner equity", "equity")
  await addAccount(request, w.api, "1500", "Fixtures", "asset")
  await loginOffice(page, w.login)

  // Nothing staged yet: the review says so and its button waits.
  await page.goto(`${urls.office}/settings/imports`)
  await page.getByRole("link", { name: "Opening review" }).click()
  await expect(
    page.getByText("No finished opening import yet.", { exact: false })
  ).toBeVisible()
  await expect(
    page.getByRole("button", { name: "Check opening" })
  ).toBeDisabled()

  const name = "opening-tb.csv"
  await importFile(
    page,
    "Opening trial balance",
    name,
    `Account code,Debit,Credit\n1500,100.00,0\n3000,0,90.00\n`
  )
  await expect(
    page.getByText("2 ready · 0 need a fix · 0 imported")
  ).toBeVisible()
  await page.getByRole("button", { name: "Import 2 ready rows" }).click()
  await expect(page.getByText("Import finished: 2 rows imported.")).toBeVisible(
    {
      timeout: 30_000,
    }
  )

  await page.goto(`${urls.office}/settings/imports/opening`)
  await expect(
    page.getByText("Starting the books (activation) is a later step.", {
      exact: false,
    })
  ).toBeVisible()
  await expect(
    page.getByRole("combobox", { name: "Opening trial balance" })
  ).toContainText(name)
  await page.getByRole("button", { name: "Check opening" }).click()
  await expect(page.getByText("Needs a fix")).toBeVisible()
  await expect(
    page.getByText(
      "The opening trial balance debits and credits must be equal."
    )
  ).toBeVisible()
  await expect(
    page.getByRole("region", { name: "Opening trial balance" })
  ).toContainText("100.00")
  // Review only: no button starts the books.
  await expect(
    page.getByRole("button", { name: /activate|start the books/i })
  ).toHaveCount(0)
  await expectNoRawValues(page)
})
