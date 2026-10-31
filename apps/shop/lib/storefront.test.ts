import { beforeEach, describe, expect, it, vi } from "vitest"

const api = vi.hoisted(() => ({
  storefrontProductsList: vi.fn(),
  storefrontProductsGet: vi.fn(),
}))
vi.mock("@genesy/api-client/endpoints/storefront", () => api)
vi.mock("next/navigation", () => ({
  notFound: () => {
    throw new Error("NEXT_NOT_FOUND")
  },
}))

const { getProduct, listProducts, StorefrontUnavailable } =
  await import("./storefront")

const httpError = (status: number) =>
  Object.assign(new Error(`Request failed with status code ${status}`), {
    response: { status, data: { message: "secret internal detail" } },
  })

beforeEach(() => {
  vi.resetAllMocks()
  vi.spyOn(console, "error").mockImplementation(() => {})
  vi.stubEnv("STOREFRONT_APP_KEY", "gsk_test_key")
})

describe("storefront calls", () => {
  it("send the app key as a bearer token, with a timeout and a short cache", async () => {
    api.storefrontProductsList.mockResolvedValue({ data: [], meta: {} })
    await listProducts(2)
    const [params, options] = api.storefrontProductsList.mock.calls[0]!
    expect(params).toEqual({ page: 2, page_size: 24 })
    expect(options.headers.Authorization).toBe("Bearer gsk_test_key")
    expect(options.timeout).toBeGreaterThan(0)
    // Signed file links last 10 minutes; a cached reply is never older than this.
    expect(options.fetchOptions.next.revalidate).toBeLessThanOrEqual(60)
  })

  it("turn a 404 into Next's not-found page", async () => {
    api.storefrontProductsGet.mockRejectedValue(httpError(404))
    await expect(getProduct("nope")).rejects.toThrow("NEXT_NOT_FOUND")
  })

  it.each([500, 401, 503])(
    "hide a %i behind StorefrontUnavailable, without the API body",
    async (status) => {
      api.storefrontProductsGet.mockRejectedValue(httpError(status))
      const error = await getProduct("p1").catch((e: unknown) => e)
      expect(error).toBeInstanceOf(StorefrontUnavailable)
      expect(String(error)).not.toContain("secret")
      expect(JSON.stringify(vi.mocked(console.error).mock.calls)).not.toContain(
        "secret"
      )
    }
  )

  it("treat a timeout or network error as unavailable", async () => {
    api.storefrontProductsList.mockRejectedValue(
      Object.assign(new Error("timeout of 8000ms exceeded"), {
        code: "ECONNABORTED",
      })
    )
    await expect(listProducts(1)).rejects.toBeInstanceOf(StorefrontUnavailable)
  })

  it("refuse to call without a key", async () => {
    vi.stubEnv("STOREFRONT_APP_KEY", "")
    await expect(listProducts(1)).rejects.toBeInstanceOf(StorefrontUnavailable)
    expect(api.storefrontProductsList).not.toHaveBeenCalled()
  })
})
