import type { Page } from "@playwright/test"

import { command, freshWorkspace } from "../../src/accounting"
import {
  apiAs,
  expect,
  loginOffice,
  stackAllow,
  test,
  urls,
} from "../../src/stack"
import { expectNoRawValues } from "../../src/test"

/** The detail page's "Next step" card. */
const next = (page: Page) => page.getByRole("region", { name: "Next step" })

test.describe("stock counts", () => {
  test.describe.configure({ mode: "serial" })
  let w: Awaited<ReturnType<typeof freshWorkspace>>
  let second: string
  test.beforeAll(async ({ request }) => {
    w = await freshWorkspace(request, "count")
    const { api } = w
    await api.post(api.ws("/warehouses"), {
      name: "Back store",
      default_warehouse: true,
    })
    const store = (
      await api.get<{ id: string; name: string }[]>(
        api.ws("/warehouses?page_size=100")
      )
    ).find((x) => x.name === "Back store")!
    const product = await api.post<{ product_id: string }>(
      api.ws("/products"),
      { name: "Honey jar", price: "12.50" }
    )
    await api.post(api.ws(`/products/${product.product_id}/stocks`), {
      warehouse_id: store.id,
      mutation_value: "10",
      reason: "Opening",
    })
    // A second draft on the same warehouse, refused to start while the first one counts.
    second = (
      await command<{ id: string }>(request, api, "POST", "/stocktakes", {
        payload: { warehouse_id: store.id, reason: "Second look" },
      })
    ).id
  })
  test.use({
    // eslint-disable-next-line no-empty-pattern -- Playwright reads fixture deps from the pattern
    allow: async ({}, use) =>
      use({
        ...stackAllow,
        // The second start is the case under test; the page shows it in words.
        responses: [
          ...stackAllow.responses,
          {
            url: `${urls.office}/v1/workspace/${w.api.workspace}/stocktakes/${second}/start`,
            status: 409,
          },
        ],
      }),
  })

  test("start, count, review and post a count; a second count can't start meanwhile", async ({
    page,
    request,
  }) => {
    await loginOffice(page, w.login)
    await page.goto(`${urls.office}/stock/counts`)
    await expect(
      page
        .getByRole("navigation", { name: "Stock" })
        .getByRole("link", { name: "Counts" })
    ).toBeVisible()

    await page.getByRole("button", { name: "New count" }).first().click()
    let dialog = page.getByRole("dialog")
    await dialog.getByLabel("Warehouse").selectOption({ label: "Back store" })
    await dialog.getByLabel("Reason").fill("Year end")
    await dialog.getByRole("button", { name: "Create count" }).click()
    await expect(
      page.getByRole("heading", { name: "Count of Back store" })
    ).toBeVisible()
    await expect(page.getByText("Draft", { exact: true })).toBeVisible()
    await expectNoRawValues(page)

    await next(page).getByRole("button", { name: "Start count" }).click()
    await page
      .getByRole("dialog")
      .getByRole("button", { name: "Start count" })
      .click()
    await expect(
      next(page).getByRole("status").filter({ hasText: "Count started." })
    ).toBeVisible()
    await expect(next(page)).toContainText("This warehouse is paused")
    const firstUrl = page.url()

    // The second draft on the same warehouse is refused, in words.
    await page.goto(`${urls.office}/stock/counts/${second}`)
    await next(page).getByRole("button", { name: "Start count" }).click()
    dialog = page.getByRole("dialog")
    await dialog.getByRole("button", { name: "Start count" }).click()
    await expect(dialog).toContainText(
      "Another count is already running in this warehouse."
    )
    await dialog.getByRole("button", { name: "Cancel" }).click()

    // Count, review the difference, post.
    await page.goto(firstUrl)
    await page.getByLabel("Counted for Honey jar").fill("8")
    await page.getByRole("button", { name: "Save counts" }).click()
    await expect(
      page.getByRole("status").filter({ hasText: "Counts saved." })
    ).toBeVisible()
    await next(page).getByRole("button", { name: "Post count" }).click()
    dialog = page.getByRole("dialog")
    await dialog.getByRole("button", { name: "Check differences" }).click()
    await expect(dialog).toContainText("Honey jar")
    await expect(dialog).toContainText("Difference -2")
    await dialog.getByRole("button", { name: "Post count" }).click()
    await expect(
      page.getByRole("status").filter({ hasText: "Count posted." })
    ).toBeVisible()
    await expect(next(page)).toContainText("Order holds on recalled")
    await expect(
      page.getByText("Posted", { exact: true }).first()
    ).toBeVisible()
    await expectNoRawValues(page)

    const api = await apiAs(request, w.login)
    const stock = await api.get<
      {
        stock_lvl: string
        relationships: { product: { name: string } | null }
      }[]
    >(api.ws("/products/stocks?page_size=100"))
    expect(
      stock.find((s) => s.relationships.product?.name === "Honey jar")
        ?.stock_lvl
    ).toBe("8.00")
  })

  test("a draft count is cancelled with a reason and stays as history", async ({
    page,
  }) => {
    await loginOffice(page, w.login)
    await page.goto(`${urls.office}/stock/counts/${second}`)
    await next(page).getByRole("button", { name: "Cancel count" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Reason").fill("Not needed")
    await dialog.getByRole("button", { name: "Cancel count" }).click()
    await expect(
      page.getByRole("status").filter({ hasText: "Count cancelled." })
    ).toBeVisible()
    await expect(next(page)).toContainText("This count was cancelled.")
    await page.goto(`${urls.office}/stock/counts`)
    await expect(page.getByText("Cancelled", { exact: true })).toBeVisible()
    await expectNoRawValues(page)
  })
})
