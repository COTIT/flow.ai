import { expect, openMenu, paged, test } from "./fake-api"

const product = {
  id: "p1",
  name: "AFM Honey 500g",
  product_nr: "1001",
  reference: "AFM-500",
  price_incl: "12.50",
  status: "active",
}

test.describe(() => {
  test.use({
    allow: { responses: [{ url: "/v1/workspace/w1/products", status: 500 }] },
  })

  test("a list that fails to load offers a retry that brings the rows", async ({
    page,
    api,
  }) => {
    let fail = true
    api.on("GET", "/workspace/:ws/products", () =>
      fail
        ? { status: 500, body: { message: "The server is busy." } }
        : paged([product])
    )
    await page.goto("/products")
    // TanStack retries three times before giving up, so allow for that.
    const alert = page
      .getByRole("alert")
      .filter({ hasText: "The server is busy." })
    await expect(alert).toBeVisible({ timeout: 20_000 })
    fail = false
    await alert.getByRole("button", { name: "Try again" }).click()
    await expect(
      page.getByRole("link", { name: "AFM Honey 500g" })
    ).toBeVisible()
    await expect(alert).toHaveCount(0)
  })
})

test("an empty list says so; a search that misses says no matches and clears", async ({
  page,
  api,
}) => {
  api.on("GET", "/workspace/:ws/products", ({ query }) =>
    paged(query.get("q") ? [] : [product])
  )
  await page.goto("/products?q=zzz")
  await expect(page.getByText("No matches")).toBeVisible()
  await expect(page.getByText("Nothing fits these filters.")).toBeVisible()
  await page.getByRole("button", { name: "Clear filters" }).click()
  await expect(page).toHaveURL(/\/products\??$/)
  await expect(page.getByRole("link", { name: "AFM Honey 500g" })).toBeVisible()
})

test("no rows at all is not a filter miss", async ({ page, api }) => {
  api.on("GET", "/workspace/:ws/invoices", () => paged([]))
  await page.goto("/invoices")
  await expect(page.getByText("No invoices yet.")).toBeVisible()
  await expect(page.getByRole("button", { name: "Clear filters" })).toHaveCount(
    0
  )
})

test.describe(() => {
  test.use({
    allow: { responses: [{ url: "/v1/me/workspaces", status: 503 }] },
  })

  test("the workspace list failing is shown, not a blank picker", async ({
    page,
    api,
  }) => {
    let fail = true
    api
      .on("GET", "/me/workspaces", () =>
        fail
          ? {
              status: 503,
              body: { message: "Workspaces are unavailable right now." },
            }
          : { data: [{ id: "w1", workspace_name: "ilundo-honey", display_name: "Ilundo Honey Co." }] }
      )
      .on("GET", "/workspace/:ws/products", () => paged([product]))
    await page.goto("/products")
    const alert = page
      .getByRole("alert")
      .filter({ hasText: "Workspaces are unavailable right now." })
    await expect(alert).toBeVisible({ timeout: 20_000 })
    fail = false
    await alert.getByRole("button", { name: "Try again" }).click()
    await expect(alert).toHaveCount(0)
    await openMenu(page)
    await expect(page.getByRole("combobox", { name: "Workspace" })).toHaveText(
      /Ilundo Honey Co\./
    )
  })
})
