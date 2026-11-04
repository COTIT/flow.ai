import { expect, expectFits, test, type FakeApi } from "./fake-api"

// Quick search (GET /quick-search): one card per group, each row links to its detail page.
const results = {
  products: [{ id: "p1", name: "Honey jar 500 g", product_nr: "P-0001", ean13: "8712345678906" }],
  orders: [
    {
      id: "ord-1",
      number: "SO-1042",
      reference: "PO honey",
      status: "open",
      currency: "USD",
      decimals: 2,
      total: 4850,
    },
  ],
  invoices: [{ id: "inv-1", number: "2026-0042", customer_name: "Honey House Ltd", status: "sent" }],
  lots: [{ id: "lot-1", code: "AFM-HN-2026-07", product_name: "Honey jar 500 g", public_code: "7K3M9QX2WD" }],
  relations: [{ id: "rel-1", name: "Honey House Ltd" }],
}

function searchApi(api: FakeApi, reply: Partial<typeof results> | Record<string, null> = results) {
  const asked: string[] = []
  api.on("GET", "/workspace/:ws/quick-search", ({ query }) => {
    asked.push(query.toString())
    return { data: { ...Object.fromEntries(Object.keys(results).map((k) => [k, []])), ...reply } }
  })
  return asked
}

test("results per group link to their pages; the query stays in the address", async ({
  page,
  api,
}) => {
  const asked = searchApi(api)
  await page.goto("/search?q=honey")
  await expect(page.getByRole("heading", { level: 1, name: "Search" })).toBeVisible()
  await expect(page.getByText("Results for “honey”")).toBeVisible()

  const products = page.getByRole("region", { name: "Products" })
  await expect(products.getByRole("link", { name: "Honey jar 500 g" })).toHaveAttribute(
    "href",
    "/products/p1"
  )
  await expect(products.getByText("P-0001")).toBeVisible()

  const orders = page.getByRole("region", { name: "Orders" })
  await expect(orders.getByRole("link", { name: "SO-1042" })).toHaveAttribute("href", "/orders/ord-1")
  await expect(orders.getByText("$48.50")).toBeVisible()
  await expect(orders.getByText("Open", { exact: true })).toBeVisible()

  const invoices = page.getByRole("region", { name: "Invoices" })
  await expect(invoices.getByRole("link", { name: "2026-0042" })).toHaveAttribute(
    "href",
    "/invoices/inv-1"
  )
  await expect(invoices.getByText("Honey House Ltd")).toBeVisible()

  const lots = page.getByRole("region", { name: "Batches" })
  await expect(lots.getByRole("link", { name: "AFM-HN-2026-07" })).toHaveAttribute(
    "href",
    "/lots/lot-1"
  )
  // Relations have no office page to link to.
  await expect(page.getByRole("region", { name: "Relations" })).toHaveCount(0)
  expect(asked[0]).toBe("q=honey&limit=10")
  await expectFits(page)
})

test("a group the user may not list (null) or with no rows is left out", async ({
  page,
  api,
}) => {
  searchApi(api, { ...results, orders: null, invoices: [] } as unknown as typeof results)
  await page.goto("/search?q=honey")
  await expect(page.getByRole("region", { name: "Products" })).toBeVisible()
  await expect(page.getByRole("region", { name: "Batches" })).toBeVisible()
  await expect(page.getByRole("region", { name: "Orders" })).toHaveCount(0)
  await expect(page.getByRole("region", { name: "Invoices" })).toHaveCount(0)
})

test("nothing found says so", async ({ page, api }) => {
  searchApi(api, { products: [], orders: [], invoices: [], lots: [], relations: [] })
  await page.goto("/search?q=xyz")
  await expect(page.getByText("Nothing found for “xyz”")).toBeVisible()
  await expectFits(page)
})

test("under 3 characters asks for more and calls nothing", async ({ page, api }) => {
  searchApi(api)
  await page.goto("/search?q=ho")
  await expect(
    page.getByRole("status").filter({ hasText: "Type at least 3 characters to search." })
  ).toBeVisible()
  await page.goto("/search")
  await expect(
    page.getByRole("status").filter({ hasText: "Type at least 3 characters to search." })
  ).toBeVisible()
  expect(api.callsTo("GET", "/workspace/:ws/quick-search")).toHaveLength(0)
  await expectFits(page)
})

test("searching again from the page keeps the query in the address", async ({ page, api }) => {
  test.skip((page.viewportSize()?.width ?? 0) >= 768, "a desktop searches from the top bar")
  const asked = searchApi(api)
  await page.goto("/search")
  await page.getByRole("searchbox", { name: "Quick search" }).fill("honey jar")
  await page.keyboard.press("Enter")
  await expect(page).toHaveURL(/\/search\?q=honey\+jar$/)
  await expect(page.getByRole("region", { name: "Products" })).toBeVisible()
  await expect(page.getByRole("searchbox", { name: "Quick search" })).toHaveValue("honey jar")
  expect(asked.at(-1)).toBe("q=honey+jar&limit=10")
})
