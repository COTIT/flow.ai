import { renderToStaticMarkup } from "react-dom/server"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { readFileSync } from "node:fs"

// Read at runtime: the shop image build context has no apps/api, and next build type-checks tests.
const document = JSON.parse(
  readFileSync(
    new URL(
      "../../api/tests/fixtures/storefront-design-v1.json",
      import.meta.url
    ),
    "utf8"
  )
)

const mocks = vi.hoisted(() => ({ design: vi.fn(), redirect: vi.fn() }))
vi.mock("next/navigation", () => ({ redirect: mocks.redirect }))
vi.mock("next-intl/server", () => ({
  getLocale: async () => "en",
  getTranslations: async () => (key: string) => key,
}))
vi.mock("@/lib/storefront-design", () => ({ getPublishedDesign: mocks.design }))
vi.mock("@/components/legacy-home", () => ({
  default: () => <div>Exact legacy body</div>,
}))
vi.mock("@/components/trace-form", () => ({
  TraceForm: () => <form aria-label="Native batch lookup" />,
}))
import HomePage from "./page"

beforeEach(() => {
  vi.resetAllMocks()
  mocks.redirect.mockImplementation(() => {
    throw new Error("redirected")
  })
})

describe("home design selection", () => {
  it("redirects old catalog links before any design lookup", async () => {
    await expect(
      HomePage({ searchParams: Promise.resolve({ page: "2&x" }) })
    ).rejects.toThrow("redirected")
    expect(mocks.redirect).toHaveBeenCalledWith("/shop?page=2%26x")
    expect(mocks.design).not.toHaveBeenCalled()
  })

  it("uses the unchanged legacy component when no supported publication is available", async () => {
    mocks.design.mockResolvedValue(null)
    const html = renderToStaticMarkup(
      await HomePage({ searchParams: Promise.resolve({}) })
    )
    expect(html).toBe("<div>Exact legacy body</div>")
  })

  it("renders published sections and keeps native batch lookup outside the document", async () => {
    mocks.design.mockResolvedValue({
      document,
      collections: [{ section_id: "featured-products", products: [] }],
    })
    const html = renderToStaticMarkup(
      await HomePage({ searchParams: Promise.resolve({}) })
    )
    expect(html).toContain("From farm to your table")
    expect(html).toContain('aria-label="Native batch lookup"')
    expect(html).not.toContain("Exact legacy body")
  })
})
