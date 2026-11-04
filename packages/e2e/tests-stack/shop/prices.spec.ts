import { expect, test, urls } from "../../src/stack"
import { expectNoRawValues } from "../../src/test"
import { ownProduct, storefront } from "./shop"

type Variant = {
  id: string
  is_default: boolean
  price_gross: number
  options: { type: string; value: string }[]
}
type Product = {
  id: string
  name: string
  price_gross: number
  currency: string
  currency_decimals: number
  variants: Variant[]
}

const money = (p: Product, minor: number) =>
  new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: p.currency,
  }).format(minor / 10 ** p.currency_decimals)
/** What the shop shows before a size is picked: the default size's price, else the product's. */
const shown = (p: Product) =>
  (p.variants.find((v) => v.is_default) ?? p.variants[0])?.price_gross ??
  p.price_gross

test("every catalogue card on page 1 shows the API's price incl. VAT", async ({
  page,
  request,
}) => {
  const products = await storefront(request)<Product[]>(
    "/products?page=1&page_size=24"
  )
  await page.goto(urls.shop)
  for (const p of products) {
    const card = page
      .getByRole("listitem")
      .filter({ has: page.getByText(p.name, { exact: true }) })
    await expect(card, p.name).toContainText(money(p, shown(p)))
  }
  await expect(page.getByRole("main")).not.toContainText("$0.00")
  await expectNoRawValues(page)
})

test("a single-price product: the product page price is the API's incl. VAT", async ({
  page,
  request,
}) => {
  const { id, name } = await ownProduct(request, { price: "4.00" })
  const p = await storefront(request)<Product>(`/products/${id}`)
  expect(p.variants).toHaveLength(1)
  await page.goto(`${urls.shop}/products/${id}`)
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(name)
  await expect(page.getByRole("main")).toContainText(money(p, p.price_gross))
  await expect(page.getByRole("group", { name: "Size" })).toHaveCount(0)
  await expectNoRawValues(page)
})

test("a product sold by size: each size button shows its API price, and picking one changes the price", async ({
  page,
  request,
}) => {
  const products = await storefront(request)<Product[]>(
    "/products?page=1&page_size=24"
  )
  const p = products.find(
    (x) => x.variants.filter((v) => v.options.length).length > 1
  )!
  await page.goto(`${urls.shop}/products/${p.id}`)
  const sizes = page.getByRole("group", { name: "Size" })
  const named = p.variants.filter((v) => v.options.length)
  for (const v of named)
    await expect(
      sizes.getByRole("button", {
        name: `${v.options[0]!.value} ${money(p, v.price_gross)}`,
      })
    ).toBeVisible()

  const big = named.reduce((a, b) => (b.price_gross > a.price_gross ? b : a))
  const price = page.getByText(/^\$[\d,]+\.\d\d$/).first()
  await sizes
    .getByRole("button", { name: new RegExp(`^${big.options[0]!.value} `) })
    .click()
  await expect(price).toHaveText(money(p, big.price_gross))
  await expectNoRawValues(page)
})

test("a product sold by size shows no nameless size", async ({
  page,
  request,
}) => {
  const products = await storefront(request)<Product[]>(
    "/products?page=1&page_size=24"
  )
  const p = products.find((x) => x.variants.length > 1)!
  await page.goto(`${urls.shop}/products/${p.id}`)
  const buttons = page.getByRole("group", { name: "Size" }).getByRole("button")
  await expect(buttons).toHaveCount(
    p.variants.filter((v) => v.options.length).length
  )
  for (const name of await buttons.allTextContents())
    expect(name).toMatch(/^\s*\S+.*\s\$/)
})
