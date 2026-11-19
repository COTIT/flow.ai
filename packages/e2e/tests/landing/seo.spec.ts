import { expect, test } from "../../src/test"

for (const [path, canonical] of [
  ["/", "https://genesy.app"],
  ["/nl", "https://genesy.app/nl"],
  ["/sw", "https://genesy.app/sw"],
] as const)
  test(`${path} names its canonical address and its other languages`, async ({
    page,
  }) => {
    await page.goto(path)
    await expect(page.locator('link[rel="canonical"]')).toHaveAttribute(
      "href",
      canonical
    )
    for (const [lang, href] of [
      ["en", "https://genesy.app"],
      ["nl", "https://genesy.app/nl"],
      ["sw", "https://genesy.app/sw"],
      ["x-default", "https://genesy.app"],
    ] as const)
      await expect(
        page.locator(`link[rel="alternate"][hreflang="${lang}"]`)
      ).toHaveAttribute("href", href)
    await expect(page.locator('meta[name="description"]')).toHaveAttribute(
      "content",
      /.{40,}/
    )
    await expect(page.locator('meta[name="robots"]')).toHaveCount(0)
  })

for (const [path, alt] of [
  ["/", "Verzo. Turn your intentions into actions."],
  ["/nl", "Verzo. Zet uw intenties om in acties."],
  ["/sw", "Verzo. Geuza nia zako kuwa vitendo."],
] as const) {
  test(`${path} serves its localized Verzo share image`, async ({
    page,
    request,
  }) => {
    await page.goto(path)
    await expect(page.locator('meta[property="og:image:alt"]')).toHaveAttribute(
      "content",
      alt
    )
    const url = await page
      .locator('meta[property="og:image"]')
      .getAttribute("content")
    expect(url).toBeTruthy()
    const response = await request.get(url!)
    expect(response.status()).toBe(200)
    expect(response.headers()["content-type"]).toBe("image/png")
  })
}

test("sitemap and robots point search engines at the three languages", async ({
  request,
}) => {
  const sitemap = await request.get("/sitemap.xml")
  expect(sitemap.status()).toBe(200)
  const xml = await sitemap.text()
  for (const url of [
    "https://genesy.app",
    "https://genesy.app/nl",
    "https://genesy.app/sw",
  ])
    expect(xml).toContain(`<loc>${url}</loc>`)
  const robots = await request.get("/robots.txt")
  expect(robots.status()).toBe(200)
  const txt = await robots.text()
  expect(txt).toContain("Allow: /")
  expect(txt).toContain("Sitemap: https://genesy.app/sitemap.xml")
})
