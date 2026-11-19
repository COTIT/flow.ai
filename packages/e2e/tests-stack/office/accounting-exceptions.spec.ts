import { freshWorkspace } from "../../src/accounting"
import { expect, loginOffice, stackAllow, test, urls } from "../../src/stack"
import { expectNoHorizontalScroll } from "../../src/test"

// Exceptions come only from provider callbacks, which the stack can't produce; retry and resolve
// are covered by tests/office/accounting-exceptions.spec.ts. This proves the real reads and states.
const unknown = "00000000-0000-7000-8000-000000000000"

test.describe("accounting exceptions", () => {
  let w: Awaited<ReturnType<typeof freshWorkspace>>
  test.beforeAll(async ({ request }) => {
    w = await freshWorkspace(request, "exceptions")
  })
  test.use({
    // eslint-disable-next-line no-empty-pattern -- Playwright reads fixture deps from the pattern
    allow: async ({}, use) =>
      use({
        ...stackAllow,
        responses: [
          ...stackAllow.responses,
          {
            url: `${urls.office}/v1/workspace/${w.api.workspace}/accounting/exceptions/${unknown}`,
            status: 404,
          },
        ],
      }),
  })

  test("nothing waiting, nothing at all, and an unknown exception, each said in words", async ({
    page,
  }) => {
    await loginOffice(page, w.login)
    await page.goto(`${urls.office}/accounting/exceptions`)
    const sections = page.getByRole("navigation", {
      name: "Accounting sections",
    })
    await expect(
      sections.getByRole("link", { name: "Exceptions" })
    ).toHaveAttribute("aria-current", "page")
    await expect(
      page.getByText("Nothing waiting. All provider money is booked.")
    ).toBeVisible()
    await page.getByRole("link", { name: "All", exact: true }).click()
    await expect(page.getByText("No exceptions yet")).toBeVisible()
    await expectNoHorizontalScroll(page)
    await page.goto(`${urls.office}/accounting/exceptions/${unknown}`)
    await expect(page.getByText("Not found", { exact: true })).toBeVisible()
    await page.getByRole("link", { name: "Back to the list" }).click()
    await expect(
      page.getByRole("heading", { name: "Exceptions", exact: true })
    ).toBeVisible()
  })
})
