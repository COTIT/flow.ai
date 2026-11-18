import { beforeEach, describe, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({
  base: "",
  listCategories: vi.fn(),
  listProducts: vi.fn(),
}))
vi.mock("@/lib/shop-scope", () => ({
  getShop: async () => null,
  getShopBase: async () => mocks.base,
}))
vi.mock("@/lib/storefront", () => ({
  listCategories: mocks.listCategories,
  listProducts: mocks.listProducts,
}))
vi.mock("next-intl/server", () => ({
  getTranslations: async () => (key: string) => key,
}))

import { generateMetadata } from "./page"

const category = {
  id: "c1000000-0000-4000-8000-000000000002",
  name: "Coffee",
  description: "Coffee grown in East Africa",
}

beforeEach(() => {
  vi.resetAllMocks()
  vi.stubEnv("SITE_URL", "https://shop.example")
  vi.stubEnv("NOINDEX", "")
  mocks.base = ""
  mocks.listCategories.mockResolvedValue([category])
  mocks.listProducts.mockImplementation(async (page: number) => ({
    data: page <= 2 ? [{}] : [],
    meta: { current_page: page, last_page: 2 },
  }))
})

describe("catalogue metadata", () => {
  it.each([
    [{}, "/shop"],
    [{ page: "1", source: "tracking" }, "/shop"],
    [{ page: "2" }, "/shop?page=2"],
    [{ category: category.id, page: "01" }, `/shop?category=${category.id}`],
    [
      { category: category.id, page: "2" },
      `/shop?category=${category.id}&page=2`,
    ],
  ])("canonicalizes populated view %j", async (params, expectedPath) => {
    const metadata = await generateMetadata({
      searchParams: Promise.resolve(params),
    })
    expect(metadata.alternates?.canonical).toBe(
      `https://shop.example${expectedPath}`
    )
    expect(metadata.robots).toBeUndefined()
  })

  it.each([{ category: "missing" }, { page: "3" }, { page: "2x" }])(
    "excludes invalid or past-end view %j",
    async (params) => {
      const metadata = await generateMetadata({
        searchParams: Promise.resolve(params),
      })
      expect(metadata.alternates).toBeUndefined()
      expect(metadata.robots).toEqual({ index: false, follow: true })
    }
  )

  it("excludes a valid but empty view", async () => {
    mocks.listProducts.mockResolvedValueOnce({
      data: [],
      meta: { current_page: 1, last_page: 1 },
    })
    await expect(
      generateMetadata({
        searchParams: Promise.resolve({ category: category.id }),
      })
    ).resolves.toEqual({
      title: "Coffee",
      robots: { index: false, follow: true },
    })
  })

  it("honors staging noindex on a populated view", async () => {
    vi.stubEnv("NOINDEX", "1")
    const metadata = await generateMetadata({
      searchParams: Promise.resolve({}),
    })
    expect(metadata.robots).toEqual({ index: false, follow: false })
  })

  it("keeps scoped metadata and skips the default catalogue check", async () => {
    mocks.base = "/s/20000000-0000-4000-8000-000000000002"
    const metadata = await generateMetadata({
      searchParams: Promise.resolve({ category: category.id }),
    })
    expect(metadata).toEqual({ title: "Coffee" })
    expect(mocks.listProducts).not.toHaveBeenCalled()
  })
})
