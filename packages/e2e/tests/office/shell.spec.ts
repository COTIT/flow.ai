import { expectNoHorizontalScroll } from "../../src/test"
import { expect, paged, test, WS, WS2 } from "./fake-api"

const products = Array.from({ length: 120 }, (_, i) => ({
  id: `p-${i + 1}`,
  name: `Honey jar ${i + 1}`,
  product_nr: `${1000 + i}`,
  reference: `AFM-${i + 1}`,
  price_incl: "12.50",
  status: "active",
}))

test.beforeEach(({ api }) => {
  api.on("GET", "/workspace/:ws/products", ({ params, query }) => {
    const q = query.get("q") ?? ""
    const rows =
      params[0] === WS ? products.filter((p) => p.name.includes(q)) : []
    return paged(rows, Number(query.get("page") ?? 1))
  })
})

test("home goes to products; list pages through meta", async ({ page }) => {
  await page.goto("/")
  await expect(page).toHaveURL(/\/products$/)
  await expect(page.getByRole("link", { name: "Products" })).toHaveAttribute(
    "aria-current",
    "page"
  )
  await expect(
    page.getByRole("cell", { name: "Honey jar 1", exact: true })
  ).toBeVisible()
  await expect(page.getByText("1–50 of 120")).toBeVisible()

  await page.getByRole("button", { name: "Next" }).click()
  await expect(page).toHaveURL(/page=2/)
  await expect(page.getByText("51–100 of 120")).toBeVisible()
  await page.getByRole("button", { name: "Next" }).click()
  await expect(page.getByText("101–120 of 120")).toBeVisible()
  await expect(page.getByRole("button", { name: "Next" })).toBeDisabled()
})

test("search sends q and resets to page 1", async ({ page, api }) => {
  await page.goto("/products?page=2")
  await page.getByRole("searchbox", { name: "Search products" }).fill("jar 7")
  await page.keyboard.press("Enter")
  await expect(page).toHaveURL(/\?q=jar\+7$/)
  await expect(page.getByText("1–11 of 11")).toBeVisible()
  expect(api.calls.at(-1)?.path).toBe(`/workspace/${WS}/products`)
})

test("switching workspace refetches for that workspace only", async ({
  page,
  api,
}) => {
  await page.goto("/products")
  await expect(page.getByText("1–50 of 120")).toBeVisible()
  await page.getByRole("combobox", { name: "Workspace" }).selectOption(WS2)
  await expect(page.getByText("No products yet.")).toBeVisible()
  expect(api.callsTo("GET", `/workspace/${WS2}/products`)).toHaveLength(1)
})

test("logout returns to login", async ({ page }) => {
  await page.goto("/products")
  await page.getByRole("button", { name: "Log out" }).click()
  await expect(page).toHaveURL(/\/login$/)
})

test("signed-out visitor is sent to login", async ({ page }) => {
  await page.addInitScript(() => localStorage.removeItem("genesy.session"))
  await page.goto("/products")
  await expect(page).toHaveURL(/\/login$/)
})

test("no sideways scroll", async ({ page }) => {
  await page.goto("/products")
  await expect(page.getByText("1–50 of 120")).toBeVisible()
  await expectNoHorizontalScroll(page)
})
