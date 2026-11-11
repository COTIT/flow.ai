import design from "../../../../apps/api/tests/fixtures/storefront-design-v1.json" with { type: "json" }
import { PREVIEW } from "../../mock/fixtures.mjs"
import type { Page, Request, Response } from "@playwright/test"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"

/** Every browser request the page makes, to prove the preview body never does commerce. */
function record(page: Page) {
  const seen: Request[] = []
  page.on("request", (r) => seen.push(r))
  return seen
}

/** Each header once, with one value: no global rule doubling or weakening the preview's. */
async function expectPreviewHeaders(response: Response | null) {
  const all = await response!.headersArray()
  const values = (name: string) =>
    all.filter((h) => h.name.toLowerCase() === name).map((h) => h.value)
  expect(values("referrer-policy")).toEqual(["no-referrer"])
  expect(values("x-robots-tag")).toEqual(["noindex, nofollow"])
  expect(values("cache-control")).toHaveLength(1)
  expect(values("cache-control")[0]).toContain("no-store")
  expect(values("x-content-type-options")).toEqual(["nosniff"])
}

function expectNoCommerce(seen: Request[]) {
  expect(seen.filter((r) => r.method() !== "GET").map((r) => r.url())).toEqual(
    []
  )
  expect(
    seen
      .map((r) => new URL(r.url()).pathname)
      .filter((p) => /^\/v1\/|\/(orders|checkout|payment)/.test(p))
  ).toEqual([])
}

test("valid preview link shows the draft in the real shop, inert and unindexed", async ({
  page,
}, testInfo) => {
  const seen = record(page)
  const response = await page.goto(`/preview/${PREVIEW.live}`)
  await expectPreviewHeaders(response)
  await expect(page.locator('meta[name="robots"]')).toHaveAttribute(
    "content",
    "noindex, nofollow"
  )

  const banner = page
    .getByRole("status")
    .filter({ hasText: "Unpublished preview" })
  await expect(banner).toContainText("Revision 019a6df0")
  await expect(banner.locator("time")).toHaveAttribute(
    "datetime",
    "2026-11-13T12:00:00Z"
  )
  await expect(banner).toContainText("Expires")
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(
    "Draft heading only in preview"
  )
  await expect(
    page.getByRole("heading", { name: design.sections[1]!.props.heading })
  ).toBeVisible()
  await expect(page.getByText("Ilundo Wild Forest Honey").first()).toBeVisible()
  // Inert body: products are not links, no hero CTA, no batch lookup form.
  const main = page.getByRole("main")
  await expect(main.getByRole("link", { name: /Ilundo/ })).toHaveCount(0)
  await expect(
    main.getByRole("link", { name: "Shop the collection" })
  ).toHaveCount(0)
  await expect(page.getByLabel("Batch code")).toHaveCount(0)
  await expect(main.locator("form")).toHaveCount(0)
  // Real shop chrome stays.
  await expect(page.getByRole("banner")).toBeVisible()

  await expectNoHorizontalScroll(page)
  await page.screenshot({
    path: testInfo.outputPath("preview.png"),
    fullPage: true,
  })
  await page.mouse.wheel(0, 4000)
  await page.waitForLoadState("networkidle")
  expectNoCommerce(seen)
})

for (const [name, token] of [
  ["expired or revoked", PREVIEW.dead],
  ["malformed", "not-a-token"],
] as const) {
  test(`${name} preview link shows the expired page`, async ({ page }) => {
    const seen = record(page)
    const response = await page.goto(`/preview/${token}`)
    await expectPreviewHeaders(response)
    await expect(
      page.getByRole("heading", { level: 1, name: "This preview has expired" })
    ).toBeVisible()
    await expect(
      page.getByText(
        "The unpublished storefront revision is still available in your workspace."
      )
    ).toBeVisible()
    await expect(
      page.getByRole("link", { name: "Return to Genesy Flow" })
    ).toHaveAttribute("href", "https://flow.example.test")
    await expect(page.getByText("Unpublished preview")).toHaveCount(0)
    await expectNoHorizontalScroll(page)
    await page.waitForLoadState("networkidle")
    expectNoCommerce(seen)
  })
}

test.describe("outside /preview", () => {
  // A path that only starts like /preview: the preview rule must not reach it.
  test.use({
    allow: {
      responses: [{ url: "/previews-are-not-preview-links", status: 404 }],
    },
  })

  test("pages outside /preview keep the shared referrer policy", async ({
    page,
  }) => {
    for (const path of ["/", "/shop", "/previews-are-not-preview-links"]) {
      const response = await page.goto(path)
      const all = await response!.headersArray()
      expect(
        all
          .filter((h) => h.name.toLowerCase() === "referrer-policy")
          .map((h) => h.value),
        path
      ).toEqual(["strict-origin-when-cross-origin"])
    }
  })
})
