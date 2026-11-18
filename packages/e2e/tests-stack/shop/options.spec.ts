import { expect, test, urls } from "../../src/stack"
import { expectNoHorizontalScroll } from "../../src/test"
import { storefront } from "./shop"

// The seed's real AFM catalogue: a gift basket with two options (13 colours x 9 spices) and an
// art print with one. One picker per option; labels stay inside their buttons.

type Listed = { id: string; name: string }

async function productId(
  request: Parameters<typeof storefront>[0],
  name: string
) {
  const get = storefront(request)
  for (let page = 1; page <= 5; page++) {
    const list = await get<Listed[]>(`/products?page=${page}&page_size=100`)
    const hit = list.find((p) => p.name === name)
    if (hit) return hit.id
    if (list.length < 100) break
  }
  throw new Error(`the seed has no product ${name}`)
}

const spilling = (buttons: import("@playwright/test").Locator) =>
  buttons.evaluateAll(
    (all) => all.filter((b) => b.scrollWidth > b.clientWidth + 1).length
  )

test("a two-option gift basket: pick a colour and a spice, the cart line names both", async ({
  page,
  request,
}) => {
  const id = await productId(request, "Holiday Gift Basket | The Tea Tradition")
  await page.goto(`${urls.shop}/products/${id}`)
  const colour = page.getByRole("group", { name: /^Basket colour/ })
  const spice = page.getByRole("group", { name: /^Spice/ })
  await expect(colour.getByRole("button")).toHaveCount(13)
  await expect(spice.getByRole("button")).toHaveCount(9)
  await expectNoHorizontalScroll(page)
  expect(await spilling(colour.getByRole("button"))).toBe(0)
  expect(await spilling(spice.getByRole("button"))).toBe(0)

  await colour.getByRole("button", { name: "Lamu Indigo Dark Blue" }).click()
  await spice.getByRole("button", { name: "Whole Cloves" }).click()
  await expect(colour).toHaveAccessibleName(
    "Basket colour: Lamu Indigo Dark Blue"
  )
  await expect(spice).toHaveAccessibleName("Spice: Whole Cloves")
  await expect(page.getByTestId("buy-price")).toHaveText(/^\$\d+\.\d{2}$/)

  await page.getByRole("button", { name: "Add to cart" }).click()
  await page.goto(`${urls.shop}/cart`)
  await expect(
    page.getByText("Basket colour: Lamu Indigo Dark Blue, Spice: Whole Cloves")
  ).toBeVisible()
})

test("a one-option print keeps one picker, every name inside its button", async ({
  page,
  request,
}) => {
  const id = await productId(request, "14 X 11 Mary Artwork Prints")
  await page.goto(`${urls.shop}/products/${id}`)
  const print = page.getByRole("group", { name: /^Print/ })
  await expect(print.getByRole("button")).toHaveCount(4)
  await expectNoHorizontalScroll(page)
  expect(await spilling(print.getByRole("button"))).toBe(0)
  const light = print.getByRole("button", { name: /^Light/ })
  await light.click()
  await expect(light).toHaveAttribute("aria-pressed", "true")
})
