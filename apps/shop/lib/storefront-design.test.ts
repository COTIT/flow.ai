vi.mock("@/lib/shop-scope", () => ({
  getShopBase: async () => "",
  getShop: async () => null,
}))
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

const api = vi.hoisted(() => ({ apiRequest: vi.fn() }))
vi.mock("@genesy/api-client/mutator", async (real) => ({
  ...(await real<object>()),
  ...api,
}))

const { getPreviewDesign, getPublishedDesign } = await import("./storefront-design")

beforeEach(() => {
  vi.resetAllMocks()
  vi.stubEnv("SHOP_DESIGN_ENABLED", "true")
  vi.stubEnv("STOREFRONT_APP_KEY", "gsk_test_key")
  vi.spyOn(console, "warn").mockImplementation(() => {})
})

describe("published design lookup", () => {
  it("returns a supported publication without fetching catalog data again", async () => {
    const publication = {
      revision_id: "f1000000-0000-4000-8000-000000000001",
      document,
      collections: [{ section_id: "featured-products", products: [] }],
    }
    api.apiRequest.mockResolvedValue({ data: publication })
    await expect(getPublishedDesign()).resolves.toEqual(publication)
    expect(api.apiRequest).toHaveBeenCalledTimes(1)
    expect(console.warn).not.toHaveBeenCalled()
  })
  it.each([undefined, "false", "TRUE", "1"])(
    "makes no request unless the rollout flag is exactly true (%s)",
    async (flag) => {
      vi.stubEnv("SHOP_DESIGN_ENABLED", flag)
      await expect(getPublishedDesign()).resolves.toBeNull()
      expect(api.apiRequest).not.toHaveBeenCalled()
      expect(console.warn).not.toHaveBeenCalled()
    }
  )

  it("uses only the new endpoint with the server key, timeout and no-store", async () => {
    api.apiRequest.mockResolvedValue({ data: null })
    await expect(getPublishedDesign()).resolves.toBeNull()
    // Nothing published yet is a normal state, not a failure.
    expect(console.warn).not.toHaveBeenCalled()
    const [request, options] = api.apiRequest.mock.calls[0]!
    expect(request).toEqual({ url: "/storefront/pages/home", method: "GET" })
    expect(options.headers).toEqual({ Authorization: "Bearer gsk_test_key" })
    expect(options.timeout).toBe(2000)
    expect(options.adapter).toBe("fetch")
    expect(options.fetchOptions).toEqual({ cache: "no-store" })
  })

  it("keeps the legacy home when no app key is configured", async () => {
    vi.stubEnv("STOREFRONT_APP_KEY", "")
    await expect(getPublishedDesign()).resolves.toBeNull()
    expect(api.apiRequest).not.toHaveBeenCalled()
    expect(console.warn).toHaveBeenCalledExactlyOnceWith(
      "storefront: using legacy home:",
      "no app key"
    )
  })

  it("keeps the legacy home on unsupported or malformed publications", async () => {
    api.apiRequest.mockResolvedValue({
      data: { document: { schema_version: 2 } },
    })
    await expect(getPublishedDesign()).resolves.toBeNull()
    expect(console.warn).toHaveBeenCalledExactlyOnceWith(
      "storefront: using legacy home:",
      "unsupported design"
    )
  })

  it.each([404, 401, 503])("falls back safely on HTTP %s", async (status) => {
    api.apiRequest.mockRejectedValue({
      response: { status, data: "secret body" },
      config: { headers: { Authorization: "Bearer gsk_test_key" } },
    })
    await expect(getPublishedDesign()).resolves.toBeNull()
    expect(console.warn).toHaveBeenCalledExactlyOnceWith(
      "storefront: using legacy home:",
      `HTTP ${status}`
    )
    expect(JSON.stringify(vi.mocked(console.warn).mock.calls)).not.toMatch(
      /secret body|gsk_test_key|Authorization|Bearer/
    )
  })

  it("falls back safely on network failure", async () => {
    api.apiRequest.mockRejectedValue(new Error("secret transport details"))
    await expect(getPublishedDesign()).resolves.toBeNull()
    expect(console.warn).toHaveBeenCalledExactlyOnceWith(
      "storefront: using legacy home:",
      "network"
    )
    expect(JSON.stringify(vi.mocked(console.warn).mock.calls)).not.toContain(
      "secret transport details"
    )
  })
})

describe("preview link lookup", () => {
  const token = "ab".repeat(32)
  const preview = {
    revision_id: "f1000000-0000-4000-8000-000000000002",
    expires_at: "2026-11-13T12:00:00Z",
    document,
    collections: [{ section_id: "featured-products", products: [] }],
  }

  it("posts the token in the body with the server key, timeout and no-store", async () => {
    vi.stubEnv("SHOP_DESIGN_ENABLED", "false")
    api.apiRequest.mockResolvedValue({ data: preview })
    await expect(getPreviewDesign(token)).resolves.toEqual(preview)
    const [request, options] = api.apiRequest.mock.calls[0]!
    expect(request).toEqual({
      url: "/storefront/preview",
      method: "POST",
      data: { token },
    })
    expect(options.headers).toEqual({ Authorization: "Bearer gsk_test_key" })
    expect(options.timeout).toBe(2000)
    expect(options.fetchOptions).toEqual({ cache: "no-store" })
  })

  it.each(["", "ab", "AB".repeat(32), "zz".repeat(32), `${token}0`, "../x"])(
    "never calls the API for a token of the wrong shape (%s)",
    async (bad) => {
      await expect(getPreviewDesign(bad)).resolves.toBeNull()
      expect(api.apiRequest).not.toHaveBeenCalled()
    }
  )

  it("shows nothing without an app key", async () => {
    vi.stubEnv("STOREFRONT_APP_KEY", "")
    await expect(getPreviewDesign(token)).resolves.toBeNull()
    expect(api.apiRequest).not.toHaveBeenCalled()
  })

  it.each([
    [{ response: { status: 404 } }, "HTTP 404"],
    [new Error("socket"), "network"],
  ])("returns null on failure and never logs the token", async (error, reason) => {
    api.apiRequest.mockRejectedValue(error)
    await expect(getPreviewDesign(token)).resolves.toBeNull()
    const logged = JSON.stringify(vi.mocked(console.warn).mock.calls)
    expect(logged).toContain(reason)
    expect(logged).not.toContain(token)
  })

  it("returns null for a document the renderer cannot show", async () => {
    api.apiRequest.mockResolvedValue({
      data: { ...preview, document: { ...document, schema_version: 99 } },
    })
    await expect(getPreviewDesign(token)).resolves.toBeNull()
  })
})
