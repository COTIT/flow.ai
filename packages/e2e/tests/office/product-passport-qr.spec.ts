import { PRODUCT, SHOP_CONNECTIONS } from "../../mock/fixtures.mjs"
import { expect, paged, test, type FakeApi } from "./fake-api"

const product = {
  id: PRODUCT.honey,
  name: "AFM Honey 500g",
  status: "active",
  archived: 0,
  store_only: false,
  labels: [],
  last_updated_at: "x",
}

function mockProduct(api: FakeApi, data: typeof product, active: boolean) {
  api
    .on("GET", "/workspace/:ws/products/:id", () => ({ data }))
    .on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => ({
      data: [],
    }))
    .on("GET", "/workspace/:ws/products/:id/stock-movements", () => paged([]))
    .on("GET", "/workspace/:ws/products/:id/images", () => ({ data: [] }))
    .on("GET", "/workspace/:ws/files", () => paged([]))
    .on("GET", "/workspace/:ws/storefront/connection", () => ({
      data: { id: SHOP_CONNECTIONS[0], active, name: "Ilundo Honey Co." },
    }))
}

test("prints a QR label for the scoped product passport", async ({
  page,
  api,
}) => {
  mockProduct(api, product, true)
  await page.addInitScript(() => {
    window.print = () => {
      document.body.dataset.printRequested = "true"
    }
  })
  await page.goto(`/products/${product.id}`)
  const tile = page.getByRole("region", { name: "Passport QR code" })
  const href = await tile.getByRole("link").getAttribute("href")
  const destination = new URL(href!)
  expect(destination.pathname).toBe(
    `/s/${SHOP_CONNECTIONS[0]}/products/${product.id}`
  )
  expect(destination.hash).toBe("#jar-passport")
  const qr = tile.getByRole("img", { name: `QR code for ${product.name}` })
  const modules = await qr.locator("path").getAttribute("d")
  expect(modules).toMatch(/^M\d+ \d+h1v1H\d+z/)
  const label = page.locator(".product-passport-print")
  await expect(label).toBeHidden()
  await tile.getByRole("button", { name: "Print QR code" }).click()
  await expect(page.locator("body")).toHaveAttribute(
    "data-print-requested",
    "true"
  )
  await page.emulateMedia({ media: "print" })
  await expect(label).toBeVisible()
  await expect(label).toHaveText(product.name)
  await expect(label.locator("svg path")).toHaveAttribute("d", modules!)
  await expect(tile).toBeHidden()
  await expect(page.locator("main")).toBeHidden()
  await page.emulateMedia({ media: null })
  await expect(label).toBeHidden()
  await expect(tile).toBeVisible()
  await page.evaluate(() => window.dispatchEvent(new Event("afterprint")))
  await expect(label).toHaveCount(0)
})

for (const { kind, data, active } of [
  { kind: "disconnected shop", data: product, active: false },
  {
    kind: "store-only product",
    data: { ...product, store_only: true },
    active: true,
  },
  {
    kind: "disabled product",
    data: { ...product, status: "disabled" },
    active: true,
  },
  { kind: "archived product", data: { ...product, archived: 1 }, active: true },
]) {
  test(`does not offer a QR label for a ${kind}`, async ({ page, api }) => {
    mockProduct(api, data, active)
    await page.goto(`/products/${product.id}`)
    await expect(
      page.getByRole("heading", { name: product.name })
    ).toBeVisible()
    await expect(
      page.getByRole("region", { name: "Passport QR code" })
    ).toHaveCount(0)
  })
}
