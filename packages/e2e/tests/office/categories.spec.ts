import { expectNoRawValues } from "../../src/test"

import {
  expect,
  expectFits,
  type FakeApi,
  openMenu,
  paged,
  test,
} from "./fake-api"

type Category = {
  id: string
  name: string
  description: string | null
  products_count: number
}

const at = "2026-11-07T09:00:00Z"
const TAKEN = "The name has already been taken."

/** Categories the office can add to, rename and remove; the API's duplicate-name rule included. */
function categoryApi(api: FakeApi, rows: Category[]) {
  const taken = (name: string, except?: string) =>
    rows.some(
      (c) =>
        c.id !== except &&
        c.name.trim().toLowerCase() === name.trim().toLowerCase()
    )
  const invalid = { status: 422, body: { message: TAKEN, errors: { name: [TAKEN] } } }
  api
    .on("GET", "/workspace/:ws/erp/product-categories", () =>
      paged(rows, 1, 100)
    )
    .on("POST", "/workspace/:ws/erp/product-categories", ({ body }) => {
      const b = body as { name: string; description: string | null }
      if (taken(b.name)) return invalid
      const id = `cat-new${rows.length}`
      rows.push({ id, ...b, products_count: 0 })
      return { status: 201, body: { data: { product_category_id: id } } }
    })
    .on("POST", "/workspace/:ws/erp/product-categories/:id", ({ params, body }) => {
      const b = body as { name: string; description: string | null }
      if (taken(b.name, params[1])) return invalid
      Object.assign(rows.find((c) => c.id === params[1])!, b)
      return { message: "ok" }
    })
    .on("DELETE", "/workspace/:ws/erp/product-categories/:id", ({ params }) => {
      rows.splice(
        rows.findIndex((c) => c.id === params[1]),
        1
      )
      return { message: "ok" }
    })
  return rows
}

const seed = (): Category[] => [
  { id: "cat-tea", name: "Tea", description: "Loose leaf", products_count: 3 },
  { id: "cat-art", name: "Art", description: null, products_count: 0 },
]

test.describe("categories page", () => {
  test("lists categories with their product counts, under Sell", async ({ page, api }) => {
    categoryApi(api, seed())
    await page.goto("/products/categories")
    await expect(page.getByRole("heading", { name: "Categories" })).toBeVisible()
    const tea = page.getByRole("row").filter({ hasText: "Tea" })
    await expect(tea).toContainText("Loose leaf")
    await expect(tea).toContainText("3")
    await expect(page.getByText("2 categories")).toBeVisible()
    await openMenu(page)
    const nav = page.getByRole("navigation", { name: "Main menu" })
    await expect(nav.getByRole("link", { name: "Categories" })).toHaveAttribute(
      "aria-current",
      "page"
    )
    await expect(nav.getByRole("link", { name: "Products" })).not.toHaveAttribute(
      "aria-current",
      "page"
    )
    await page.keyboard.press("Escape")
    await expectNoRawValues(page)
    await expectFits(page)
  })

  test("a new category is added with its description", async ({ page, api }) => {
    const rows = categoryApi(api, seed())
    await page.goto("/products/categories")
    await page.getByRole("button", { name: "New category" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Name").fill("Honey")
    await dialog.getByLabel("Description").fill("From the hive")
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(page.getByRole("status").filter({ hasText: "Category created." })).toBeVisible()
    await expect(page.getByRole("row").filter({ hasText: "From the hive" })).toBeVisible()
    expect(api.callsTo("POST", "/workspace/:ws/erp/product-categories")[0]!.body).toEqual({
      name: "Honey",
      description: "From the hive",
    })
    expect(rows).toHaveLength(3)
    await expectFits(page)
  })

  test.describe(() => {
    // The fake answers the taken name with 422, which the guard would count as a failure.
    test.use({
      allow: {
        responses: [{ url: "/v1/workspace/w1/erp/product-categories", status: 422 }],
      },
    })
    test("a taken name is refused in words, and the dialog stays open", async ({ page, api }) => {
      categoryApi(api, seed())
      await page.goto("/products/categories")
      await page.getByRole("button", { name: "New category" }).click()
      const dialog = page.getByRole("dialog")
      await dialog.getByLabel("Name").fill(" tea ")
      await dialog.getByRole("button", { name: "Save" }).click()
      await expect(dialog.getByText(TAKEN).first()).toBeVisible()
      await expect(dialog.getByLabel("Name")).toHaveAttribute("aria-invalid", "true")
      await expectNoRawValues(page)
      await expectFits(page)
    })
  })

  test("Edit renames a category and changes its description", async ({ page, api }) => {
    categoryApi(api, seed())
    await page.goto("/products/categories")
    await page
      .getByRole("row")
      .filter({ hasText: "Tea" })
      .getByRole("button", { name: "Edit" })
      .click()
    const dialog = page.getByRole("dialog", { name: "Edit Tea" })
    await expect(dialog.getByLabel("Name")).toHaveValue("Tea")
    await dialog.getByLabel("Name").fill("Herbal tea")
    await dialog.getByLabel("Description").fill("")
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(page.getByRole("status").filter({ hasText: "Category saved." })).toBeVisible()
    await expect(page.getByRole("cell", { name: "Herbal tea" })).toBeVisible()
    expect(
      api.callsTo("POST", "/workspace/:ws/erp/product-categories/:id")[0]
    ).toMatchObject({
      path: "/workspace/w1/erp/product-categories/cat-tea",
      body: { name: "Herbal tea", description: null },
    })
  })

  test("Remove says how many products lose the category; Cancel sends nothing", async ({
    page,
    api,
  }) => {
    categoryApi(api, seed())
    await page.goto("/products/categories")
    const tea = page.getByRole("row").filter({ hasText: "Tea" })
    await tea.getByRole("button", { name: "Remove" }).click()
    const ask = page.getByRole("alertdialog", { name: "Remove Tea?" })
    await expect(ask).toContainText(
      "3 products will become uncategorised. The products themselves stay."
    )
    await expectFits(page)
    await ask.getByRole("button", { name: "Cancel" }).click()
    expect(api.callsTo("DELETE", "/workspace/:ws/erp/product-categories/:id")).toHaveLength(0)

    await tea.getByRole("button", { name: "Remove" }).click()
    await ask.getByRole("button", { name: "Remove" }).click()
    await expect(page.getByRole("status").filter({ hasText: "Tea removed." })).toBeVisible()
    await expect(page.getByRole("cell", { name: "Tea", exact: true })).toHaveCount(0)
    expect(api.callsTo("DELETE", "/workspace/:ws/erp/product-categories/:id")[0]!.path).toBe(
      "/workspace/w1/erp/product-categories/cat-tea"
    )
  })

  test("Remove on an unused category says no product uses it", async ({ page, api }) => {
    categoryApi(api, seed())
    await page.goto("/products/categories")
    await page
      .getByRole("row")
      .filter({ hasText: "Art" })
      .getByRole("button", { name: "Remove" })
      .click()
    await expect(page.getByRole("alertdialog")).toContainText(
      "No products use this category."
    )
  })
})

const product = (category_id: string | null) => ({
  id: "p1",
  name: "Roselle tea",
  product_nr: "PRO-00001",
  reference: null,
  status: "active",
  price: "4.0000",
  price_incl: "4.00",
  short_description: null,
  description: null,
  labels: [],
  store_only: false,
  category_id,
  last_updated_at: at,
})

/** One product page's reads, with the product in `category_id`. */
function productPage(api: FakeApi, category_id: string | null) {
  api
    .on("GET", "/workspace/:ws/products/:id", () => ({ data: product(category_id) }))
    .on("POST", "/workspace/:ws/products/:id", () => ({ message: "ok" }))
    .on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => ({ data: [] }))
    .on("GET", "/workspace/:ws/products/:id/stock-movements", () => paged([]))
    .on("GET", "/workspace/:ws/products/:id/images", () => ({ data: [] }))
    .on("GET", "/workspace/:ws/files", () => paged([]))
    .on("GET", "/workspace/:ws/warehouses", () => paged([]))
}

const saved = (api: FakeApi) =>
  api.callsTo("POST", "/workspace/:ws/products/:id")[0]!.body as Record<string, unknown>

test.describe("product page category", () => {
  test("picking a category saves it with the product", async ({ page, api }) => {
    categoryApi(api, seed())
    productPage(api, null)
    await page.goto("/products/p1")
    const picker = page.getByLabel("Category")
    await expect(picker).toHaveValue("")
    await expect(picker.locator("option")).toHaveText(["No category", "Tea", "Art"])
    await picker.selectOption({ label: "Tea" })
    await page.getByRole("button", { name: "Save changes" }).click()
    await expect(page.getByRole("status").filter({ hasText: "Product saved." })).toBeVisible()
    expect(saved(api).category_id).toBe("cat-tea")
    await expectNoRawValues(page)
    await expectFits(page)
  })

  test("No category clears it; an untouched picker sends nothing", async ({ page, api }) => {
    categoryApi(api, seed())
    productPage(api, "cat-tea")
    await page.goto("/products/p1")
    const picker = page.getByLabel("Category")
    await expect(picker).toHaveValue("cat-tea")
    await page.getByRole("button", { name: "Save changes" }).click()
    await expect(page.getByRole("status").filter({ hasText: "Product saved." })).toBeVisible()
    expect(saved(api)).not.toHaveProperty("category_id")

    await picker.selectOption({ label: "No category" })
    await page.getByRole("button", { name: "Save changes" }).click()
    await expect
      .poll(() => api.callsTo("POST", "/workspace/:ws/products/:id").length)
      .toBe(2)
    expect(api.callsTo("POST", "/workspace/:ws/products/:id")[1]!.body).toMatchObject({
      category_id: null,
    })
  })

  test.describe(() => {
    test.use({
      allow: {
        responses: [
          { url: "/v1/workspace/w1/erp/product-categories?page_size=100", status: 500 },
        ],
      },
    })
    test("a list that fails to load leaves the category as it is", async ({ page, api }) => {
      productPage(api, "cat-tea")
      api.on("GET", "/workspace/:ws/erp/product-categories", () => ({
        status: 500,
        body: { message: "The server is busy." },
      }))
      await page.goto("/products/p1")
      await expect(
        page.getByText("Categories could not be loaded, so the category stays as it is.")
      ).toBeVisible({ timeout: 20_000 })
      await expect(page.getByLabel("Category")).toBeDisabled()
      await page.getByRole("button", { name: "Save changes" }).click()
      await expect(page.getByRole("status").filter({ hasText: "Product saved." })).toBeVisible()
      expect(saved(api)).not.toHaveProperty("category_id")
    })
  })
})

test.describe("products list and New product", () => {
  test("the list shows each product's category and filters by one", async ({ page, api }) => {
    categoryApi(api, seed())
    const asked: (string | null)[] = []
    const bao = { ...product(null), id: "p2", name: "Bao board" }
    api.on("GET", "/workspace/:ws/products", ({ query }) => {
      asked.push(query.get("category_id"))
      return paged(query.get("category_id") ? [product("cat-tea")] : [product("cat-tea"), bao])
    })
    await page.goto("/products")
    await expect(page.getByRole("row").filter({ hasText: "Roselle tea" })).toContainText("Tea")
    await expect(page.getByRole("link", { name: "Bao board" })).toBeVisible()
    await page.getByLabel("Category").selectOption({ label: "Tea" })
    await expect(page).toHaveURL(/category=cat-tea/)
    await expect(page.getByRole("link", { name: "Bao board" })).toHaveCount(0)
    expect(asked).toEqual([null, "cat-tea"])
    await expectNoRawValues(page)
    await expectFits(page)
  })

  test("New product can start in a category", async ({ page, api }) => {
    categoryApi(api, seed())
    api
      .on("GET", "/workspace/:ws/products", () => paged([]))
      .on("GET", "/workspace/:ws/settings/vat-groups", () => paged([]))
      .on("POST", "/workspace/:ws/products", () => ({
        status: 201,
        body: { data: { product_id: "p1" } },
      }))
    productPage(api, "cat-art")
    await page.goto("/products")
    await page.getByRole("button", { name: "New product" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Name").fill("Roselle tea")
    await dialog.getByLabel("Price excl. VAT").fill("4.00")
    await expect(dialog.getByLabel("Category")).toHaveValue("")
    await dialog.getByLabel("Category").selectOption({ label: "Art" })
    await expectFits(page)
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(page.getByRole("status").filter({ hasText: "Product created." })).toBeVisible()
    expect(api.callsTo("POST", "/workspace/:ws/products")[0]!.body).toMatchObject({
      name: "Roselle tea",
      category_id: "cat-art",
    })
  })
})
