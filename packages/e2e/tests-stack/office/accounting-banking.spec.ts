import type { Page } from "@playwright/test"

import { freshWorkspace } from "../../src/accounting"
import { bankStatement } from "../../src/erp"
import { expect, loginOffice, stackAllow, test, urls } from "../../src/stack"
import { expectNoHorizontalScroll, expectNoRawValues } from "../../src/test"

/** The table row of the bank line described as `text`. */
const row = (page: Page, text: string) =>
  page.getByRole("row").filter({ hasText: text })

test.describe("bank statements", () => {
  test.describe.configure({ mode: "serial" })
  let w: Awaited<ReturnType<typeof freshWorkspace>>
  let seed: Awaited<ReturnType<typeof bankStatement>>
  test.beforeAll(async ({ request }) => {
    test.setTimeout(120_000)
    w = await freshWorkspace(request, "bank")
    seed = await bankStatement(request, w.api)
  })
  test.use({
    // eslint-disable-next-line no-empty-pattern -- Playwright reads fixture deps from the pattern
    allow: async ({}, use) =>
      use({
        ...stackAllow,
        responses: [
          ...stackAllow.responses,
          // A match that doesn't add up is the refusal under test; the dialog says it in words.
          {
            url: `${urls.office}/v1/workspace/${w.api.workspace}/accounting/bank-reconciliations`,
            status: 422,
          },
        ],
      }),
  })

  test("match bank lines to payments, refuse a wrong sum, reverse a match", async ({
    page,
  }) => {
    await loginOffice(page, w.login)
    await page.goto(`${urls.office}/accounting/banking`)
    const sections = page.getByRole("navigation", {
      name: "Accounting sections",
    })
    await expect(
      sections.getByRole("link", { name: "Banking" })
    ).toHaveAttribute("aria-current", "page")
    await expect(
      page.getByRole("link", { name: "Import a statement" })
    ).toHaveAttribute("href", "/settings/imports")
    await page.getByRole("link", { name: seed.reference }).click()
    await expect(
      page.getByRole("heading", { name: seed.reference })
    ).toBeVisible()
    await expect(page.getByText("0 of 3")).toBeVisible()
    await expectNoRawValues(page, { allow: [seed.reference] })
    await expectNoHorizontalScroll(page)

    // The -40.00 line: the server offers both payments with what is left; a wrong sum is refused.
    await row(page, "Bees BV invoice 7")
      .getByRole("button", { name: "Match" })
      .click()
    let dialog = page.getByRole("dialog")
    const forty = dialog.getByRole("checkbox", { name: /40\.00 left/ })
    await expect(forty).toBeVisible()
    await expect(
      dialog.getByRole("checkbox", { name: /25\.00 left/ })
    ).toBeVisible()
    await forty.check()
    await dialog.getByLabel("Amount to match").fill("-30.00")
    await dialog.getByRole("button", { name: "Match" }).click()
    await expect(dialog).toContainText(
      "The amounts must add up to the bank line exactly."
    )
    await dialog.getByLabel("Amount to match").fill("-40.00")
    await dialog.getByRole("button", { name: "Match" }).click()
    await expect(
      page.getByRole("status").filter({ hasText: "Bank line matched." })
    ).toBeVisible()
    await expect(row(page, "Bees BV invoice 7")).toContainText("Matched")

    // The -25.00 line takes the other payment at its offered amount.
    await row(page, "Bees BV invoice 8")
      .getByRole("button", { name: "Match" })
      .click()
    dialog = page.getByRole("dialog")
    await expect(
      dialog.getByRole("checkbox", { name: /40\.00 left/ })
    ).toHaveCount(0)
    await dialog.getByRole("checkbox", { name: /25\.00 left/ }).check()
    await dialog.getByRole("button", { name: "Match" }).click()
    await expect(
      page.getByRole("status").filter({ hasText: "Bank line matched." })
    ).toBeVisible()
    await expect(page.getByText("2 of 3")).toBeVisible()

    // The fee has no payment behind it: nothing to offer.
    await row(page, "Bank fee").getByRole("button", { name: "Match" }).click()
    dialog = page.getByRole("dialog")
    await expect(dialog).toContainText(
      "No posted journal line of this bank account is left to match."
    )
    await dialog.getByRole("button", { name: "Cancel" }).click()

    // Reverse the first match: the line is free again.
    await row(page, "Bees BV invoice 7")
      .getByRole("button", { name: "Reverse match" })
      .click()
    dialog = page.getByRole("dialog")
    await dialog.getByLabel("Reason").fill("Matched the wrong payment")
    await dialog.getByRole("button", { name: "Reverse match" }).click()
    await expect(
      page.getByRole("status").filter({ hasText: "Match reversed." })
    ).toBeVisible()
    await expect(
      row(page, "Bees BV invoice 7").getByRole("button", { name: "Match" })
    ).toBeVisible()
    await expect(page.getByText("1 of 3")).toBeVisible()
    await expectNoRawValues(page, { allow: [seed.reference] })
    await expectNoHorizontalScroll(page)
  })

  test("a fresh book has no statements and says so", async ({
    page,
    request,
  }) => {
    const other = await freshWorkspace(request, "nobank")
    await loginOffice(page, other.login)
    await page.goto(`${urls.office}/accounting/banking`)
    await expect(page.getByText("No bank statements yet")).toBeVisible()
    await expectNoHorizontalScroll(page)
  })
})
