import { CATEGORY, PRODUCT, SHOP_CONNECTIONS } from "../../mock/fixtures.mjs"
import { expect, test } from "../../src/test"

test("public catalog pages expose canonical search URLs", async ({
  page,
  baseURL,
}) => {
  for (const [path, canonical] of [
    ["/", "/"],
    ["/stories", "/stories"],
    ["/shop?page=1&utm_source=example", "/shop"],
    ["/shop?page=2", "/shop?page=2"],
    [
      `/shop?category=${CATEGORY.honey}&page=1`,
      `/shop?category=${CATEGORY.honey}`,
    ],
  ]) {
    await page.goto(path)
    await expect(page.locator('link[rel="canonical"]')).toHaveJSProperty(
      "href",
      new URL(canonical, baseURL).href
    )
    await expect(page.locator('meta[name="description"]')).toHaveAttribute(
      "content",
      /\S/
    )
  }

  for (const path of [
    "/shop?page=999",
    `/shop?category=${CATEGORY.unknown}`,
    "/cart",
    "/checkout",
  ]) {
    await page.goto(path)
    await expect(page.locator('meta[name="robots"]')).toHaveAttribute(
      "content",
      /noindex/
    )
  }
})

test("product JSON-LD matches the rendered price and uses a durable image", async ({
  page,
  baseURL,
  request,
}) => {
  const path = `/products/${PRODUCT.honey}`
  const response = await page.goto(path)
  await expect(page.getByTestId("buy-price")).toHaveText("$14.99")
  const scripts = await page
    .locator('script[type="application/ld+json"]')
    .evaluateAll((elements) =>
      elements.map((element) => {
        const script = element as HTMLScriptElement
        const value = JSON.parse(script.textContent ?? "")
        return { nonce: script.nonce, entities: value["@graph"] ?? [value] }
      })
    )
  const script = scripts.find(({ entities }) =>
    entities.some((value: { "@type": string }) => value["@type"] === "Product")
  )
  expect(script?.nonce).toBeTruthy()
  expect(response?.headers()["content-security-policy"]).toContain(
    `'nonce-${script?.nonce}'`
  )
  const product = script?.entities.find(
    (value: { "@type": string }) => value["@type"] === "Product"
  )
  expect(product).toMatchObject({
    name: "Ilundo Wild Forest Honey",
    offers: {
      priceCurrency: "USD",
      availability: "https://schema.org/LimitedAvailability",
    },
  })
  expect(String(product.offers.price)).toBe("14.99")
  const imageUrl = `${baseURL}${path}/image`
  expect([product.image].flat()).toContain(imageUrl)
  await expect(page.locator('meta[property="og:image"]')).toHaveAttribute(
    "content",
    imageUrl
  )
  const image = await request.get(`${path}/image`, { maxRedirects: 0 })
  expect(image.status()).toBe(200)
  expect(image.headers()["content-type"]).toMatch(/^image\//)
  expect((await image.body()).length).toBeGreaterThan(0)

  await page.goto(`/s/${SHOP_CONNECTIONS[0]}${path}`)
  await expect(page.locator('script[type="application/ld+json"]')).toHaveCount(
    0
  )
})

test("robots advertises public sitemap shards and omits private routes", async ({
  request,
  baseURL,
}) => {
  const robots = await request.get("/robots.txt")
  expect(robots.ok()).toBeTruthy()
  expect(await robots.text()).toContain(`Sitemap: ${baseURL}/sitemap.xml`)
  const index = await request.get("/sitemap.xml")
  expect(index.ok()).toBeTruthy()
  expect(index.headers()["content-type"]).toMatch(/xml/)
  const xml = await index.text()
  expect(xml).toContain(`${baseURL}/sitemap-pages.xml`)
  expect(xml).toContain(`${baseURL}/sitemap-products-1.xml`)

  const pages = await request.get("/sitemap-pages.xml")
  const products = await request.get("/sitemap-products-1.xml")
  expect(pages.ok() && products.ok()).toBeTruthy()
  const pageXml = await pages.text()
  const productXml = await products.text()
  expect(pageXml).toContain(`${baseURL}/stories`)
  expect(pageXml).toContain(`${baseURL}/shop?category=${CATEGORY.honey}`)
  expect(productXml).toContain(`${baseURL}/products/${PRODUCT.honey}/image`)
  expect(productXml).toContain(`${baseURL}/products/${PRODUCT.wax}`)
  expect(pageXml + productXml).not.toMatch(
    /\/(?:cart|checkout|orders|preview|s)(?:\/|<)/
  )
})
