import type { Page } from "@playwright/test"

import { freshWorkspace } from "../../src/accounting"
import { draftCredit } from "../../src/erp"
import { expect, loginOffice, stackAllow, test, urls } from "../../src/stack"
import { expectNoHorizontalScroll, expectNoRawValues } from "../../src/test"

const next = (page: Page) => page.getByRole("region", { name: "Next step" })

test.describe("customer returns", () => {
  test.describe.configure({ mode: "serial" })
  let w: Awaited<ReturnType<typeof freshWorkspace>>
  let seed: Awaited<ReturnType<typeof draftCredit>>
  test.beforeAll(async ({ request }) => {
    test.setTimeout(120_000)
    w = await freshWorkspace(request, "returns")
    seed = await draftCredit(request, w.api)
  })
  test.use({
    // eslint-disable-next-line no-empty-pattern -- Playwright reads fixture deps from the pattern
    allow: async ({}, use) =>
      use({
        ...stackAllow,
        responses: [
          ...stackAllow.responses,
          // Restocking more than is left is the refusal under test; the dialog says it in words.
          {
            url: `${urls.office}/v1/workspace/${w.api.workspace}/sales-returns/${seed.credit}/restock/review`,
            status: 422,
          },
          // An unknown return: the page says it is not found.
          {
            url: `${urls.office}/v1/workspace/${w.api.workspace}/sales-returns/00000000-0000-7000-8000-000000000000`,
            status: 404,
          },
        ],
      }),
  })

  test("post a credit, restock part of it, undo the restock, reverse the credit", async ({
    page,
  }) => {
    await loginOffice(page, w.login)
    await page.goto(`${urls.office}/sales-returns`)
    await expect(
      page.getByRole("heading", { name: "Customer returns" })
    ).toBeVisible()
    await page.getByRole("link", { name: /^Return of order / }).click()
    await expect(page.getByText("Draft", { exact: true }).first()).toBeVisible()
    await expect(page.getByRole("region", { name: "Lines" })).toContainText(
      "Honey jar"
    )
    await expectNoRawValues(page)
    await expectNoHorizontalScroll(page)

    // Post: the server's review first.
    await next(page).getByRole("button", { name: "Post credit" }).click()
    let dialog = page.getByRole("dialog")
    await expect(dialog).toContainText("25.00")
    await dialog.getByRole("button", { name: "Post credit" }).click()
    await expect(
      next(page).getByRole("status").filter({ hasText: "Credit posted." })
    ).toBeVisible()
    await expect(
      page.getByText("Posted", { exact: true }).first()
    ).toBeVisible()

    // Restock: more than was returned is refused in words; one jar goes back at its cost.
    await next(page).getByRole("button", { name: "Restock" }).click()
    dialog = page.getByRole("dialog")
    const qty = dialog.getByLabel(/Honey jar into Main store/)
    await qty.fill("3")
    await dialog.getByRole("button", { name: "Review" }).click()
    await expect(dialog).toContainText(
      "Only 2 is left to put back on this line."
    )
    await qty.fill("1")
    await dialog.getByRole("button", { name: "Review" }).click()
    await expect(dialog).toContainText(
      "The original cost is put back when you restock."
    )
    await dialog.getByRole("button", { name: "Restock" }).click()
    await expect(
      next(page).getByRole("status").filter({ hasText: "Goods back in stock." })
    ).toBeVisible()
    const restocks = page.getByRole("region", { name: "Back in stock" })
    await expect(restocks).toContainText("Back into Main store")
    await expect(restocks).toContainText("Honey jar 1 (10.00)")

    // Undo that restock exactly.
    await restocks.getByRole("button", { name: "Undo restock" }).click()
    dialog = page.getByRole("dialog")
    await expect(dialog).toContainText("10.00")
    await dialog.getByLabel("Reason").fill("Jar was empty")
    await dialog.getByRole("button", { name: "Undo restock" }).click()
    await expect(
      next(page).getByRole("status").filter({ hasText: "Restock undone." })
    ).toBeVisible()
    await expect(restocks.getByText("Reversed", { exact: true })).toBeVisible()

    // Reverse the credit: review, reason, confirm.
    await next(page).getByRole("button", { name: "Reverse credit" }).click()
    dialog = page.getByRole("dialog")
    await dialog.getByLabel("Reason").fill("Customer kept the jars")
    await dialog.getByRole("button", { name: "Review" }).click()
    await expect(dialog).toContainText("Restocked goods stay in stock.")
    await dialog.getByRole("button", { name: "Reverse credit" }).click()
    await expect(
      next(page).getByRole("status").filter({ hasText: "Credit reversed." })
    ).toBeVisible()
    await expect(next(page)).toContainText("This credit was reversed.")
    await expectNoRawValues(page)
    await expectNoHorizontalScroll(page)
  })

  test("the menu leads to returns, and an unknown return says so", async ({
    page,
  }) => {
    await loginOffice(page, w.login)
    await page.goto(
      `${urls.office}/sales-returns/00000000-0000-7000-8000-000000000000`
    )
    await expect(page.getByText("Not found", { exact: true })).toBeVisible()
    await page.getByRole("link", { name: "Back to the list" }).click()
    await expect(
      page.getByRole("heading", { name: "Customer returns" })
    ).toBeVisible()
    await expect(
      page.getByText("Reversed", { exact: true }).first()
    ).toBeVisible()
    await expectNoHorizontalScroll(page)
  })
})
