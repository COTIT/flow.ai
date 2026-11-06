import { expect, notes, test, urls } from "../../src/stack"
import { expectNoHorizontalScroll, expectNoRawValues } from "../../src/test"

// The seed's published passport, as the path its QR opens ("/b/<code>").
const released = notes["passport (QR target)"]!

test("home opens on the mission and shows the seed's products in stock", async ({
  page,
}) => {
  const response = await page.goto(urls.shop)
  expect(response?.status()).toBe(200)
  await expect(page.getByRole("heading", { level: 1 })).toHaveAccessibleName(
    "Rooted in African soil. Shared with the world."
  )
  const featured = page.getByRole("region", { name: "In stock now" })
  await expect(featured.getByRole("listitem").first()).toBeVisible()
  await expect(featured.getByText("Sold out")).toHaveCount(0)
  await expectNoRawValues(page)
  await expectNoHorizontalScroll(page)
})

test("a batch code typed on home opens the real passport", async ({ page }) => {
  await page.goto(urls.shop)
  const code = released.split("/").at(-1)!
  // As people copy it off a label: lower case, a space in the middle.
  await page
    .getByLabel("Batch code")
    .fill(`${code.slice(0, 4)} ${code.slice(4)}`.toLowerCase())
  await page.getByRole("button", { name: "Show its passport" }).click()
  await expect(page).toHaveURL(`${urls.shop}${released}`, { timeout: 15_000 })
  await expect(page.getByText("Released", { exact: true })).toBeVisible()
})

test("the shop lists the seed's products, and its categories filter them", async ({
  page,
}) => {
  await page.goto(`${urls.shop}/shop`)
  const all = page.getByRole("list", { name: "All products" })
  await expect(all.getByRole("listitem").first()).toBeVisible()
  await expectNoRawValues(page)

  // Category chips show once the API lists categories; with none, the shop still works.
  const chips = page.getByRole("navigation", { name: "Categories" })
  if (!(await chips.count())) return
  const first = chips.getByRole("link").nth(1)
  const name = (await first.textContent())!
  await first.click()
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(name, {
    timeout: 15_000,
  })
  await expect(chips.getByRole("link", { name })).toHaveAttribute(
    "aria-current",
    "page"
  )
  await expect(
    page.getByRole("list", { name }).getByRole("listitem").first()
  ).toBeVisible()
})

for (const path of ["/about", "/stories", "/suppliers", "/faq", "/contact"])
  test(`${path} opens against the real stack`, async ({ page }) => {
    const response = await page.goto(`${urls.shop}${path}`)
    expect(response?.status()).toBe(200)
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
    await expectNoHorizontalScroll(page)
  })
