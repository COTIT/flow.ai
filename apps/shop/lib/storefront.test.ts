import { beforeEach, describe, expect, it, vi } from "vitest"

const api = vi.hoisted(() => ({
  storefrontProductsList: vi.fn(),
  storefrontProductsGet: vi.fn(),
  storefrontLotsGet: vi.fn(),
  storefrontOrdersCreate: vi.fn(),
  storefrontOrdersGet: vi.fn(),
  storefrontOrdersPayment: vi.fn(),
}))
vi.mock("@genesy/api-client/endpoints/storefront", () => api)
const mutator = vi.hoisted(() => ({ apiRequest: vi.fn() }))
vi.mock("@genesy/api-client/mutator", async (real) => ({
  ...(await real<object>()),
  ...mutator,
}))
vi.mock("next/navigation", () => ({
  notFound: () => {
    throw new Error("NEXT_NOT_FOUND")
  },
}))

const {
  createOrder,
  getLot,
  getOrder,
  getProduct,
  listCategories,
  listProducts,
  startPayment,
  StorefrontUnavailable,
} = await import("./storefront")

const httpError = (status: number) =>
  Object.assign(new Error(`Request failed with status code ${status}`), {
    response: { status, data: { message: "secret internal detail" } },
  })

beforeEach(() => {
  vi.resetAllMocks()
  vi.spyOn(console, "error").mockImplementation(() => {})
  vi.stubEnv("STOREFRONT_APP_KEY", "gsk_test_key")
})

describe("categories", () => {
  const coffee = "c1000000-0000-4000-8000-000000000002"

  it("lists the API's categories", async () => {
    const data = [
      { id: coffee, name: "Coffee", description: null, product_count: 3 },
    ]
    mutator.apiRequest.mockResolvedValue({ data })
    await expect(listCategories()).resolves.toEqual(data)
    expect(mutator.apiRequest.mock.calls[0]![0]).toMatchObject({
      url: "/storefront/categories",
      method: "GET",
    })
  })

  it("read as none while the API has no categories endpoint (404), so the shop still opens", async () => {
    mutator.apiRequest.mockRejectedValue(httpError(404))
    await expect(listCategories()).resolves.toEqual([])
  })

  it("hide other failures behind StorefrontUnavailable", async () => {
    mutator.apiRequest.mockRejectedValue(httpError(503))
    await expect(listCategories()).rejects.toBeInstanceOf(StorefrontUnavailable)
  })

  it("filter products by a category id, and drop anything that isn't one", async () => {
    api.storefrontProductsList.mockResolvedValue({ data: [], meta: {} })
    await listProducts(1, coffee)
    await listProducts(1, "../orders")
    expect(api.storefrontProductsList.mock.calls[0]![0]).toEqual({
      page: 1,
      page_size: 24,
      category: coffee,
    })
    expect(api.storefrontProductsList.mock.calls[1]![0]).toEqual({
      page: 1,
      page_size: 24,
    })
  })
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
    await expect(
      getProduct("00000000-0000-4000-8000-000000000000")
    ).rejects.toThrow("NEXT_NOT_FOUND")
  })

  it.each([500, 401, 503])(
    "hide a %i behind StorefrontUnavailable, without the API body",
    async (status) => {
      api.storefrontProductsGet.mockRejectedValue(httpError(status))
      const error = await getProduct(
        "11111111-1111-4111-8111-111111111111"
      ).catch((e: unknown) => e)
      expect(error).toBeInstanceOf(StorefrontUnavailable)
      expect(String(error)).not.toContain("secret")
      expect(JSON.stringify(vi.mocked(console.error).mock.calls)).not.toContain(
        "secret"
      )
    }
  )

  it("log the API path and upstream status, never the key or request headers", async () => {
    api.storefrontLotsGet.mockRejectedValue(
      Object.assign(httpError(503), {
        config: { headers: { Authorization: "Bearer gsk_test_key" } },
      })
    )
    await getLot("7K3M9QX2WD").catch(() => {})
    const logged = JSON.stringify(vi.mocked(console.error).mock.calls)
    expect(logged).toContain("/storefront/lots/7K3M9QX2WD")
    expect(logged).toContain("503")
    expect(logged).not.toMatch(/gsk_|Bearer|Authorization/)
  })

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

describe("path guards", () => {
  it("send a malformed product id or batch code to not-found without calling the API", async () => {
    await expect(getProduct("../lots/7K3M9QX2WD")).rejects.toThrow(
      "NEXT_NOT_FOUND"
    )
    await expect(getLot("../products")).rejects.toThrow("NEXT_NOT_FOUND")
    expect(api.storefrontProductsGet).not.toHaveBeenCalled()
    expect(api.storefrontLotsGet).not.toHaveBeenCalled()
  })
})

describe("getLot", () => {
  it("always asks the API, never the data cache, so a recall shows at once", async () => {
    api.storefrontLotsGet.mockResolvedValue({
      data: { public_code: "7K3M9QX2WD" },
    })
    const lot = await getLot("7K3M9QX2WD")
    expect(lot.public_code).toBe("7K3M9QX2WD")
    expect(api.storefrontLotsGet).toHaveBeenCalledTimes(1)
    expect(api.storefrontLotsGet.mock.calls[0]![1].fetchOptions).toEqual({
      cache: "no-store",
    })
  })
})

const ORDER_ID = "99999999-9999-4999-8999-999999999999"
const order = { id: ORDER_ID, number: "WEB-1", status: "awaiting_payment" }
const errorWith = (status: number, data: unknown) =>
  Object.assign(new Error(`status ${status}`), { response: { status, data } })

describe("createOrder", () => {
  const input = {
    lines: [{ product_id: "p1", product_variant_id: null, quantity: "2" }],
    customer: { email: "a@b.test", name: "A" },
  }

  it("posts the checkout with the app key and an idempotency key, never cached", async () => {
    api.storefrontOrdersCreate.mockResolvedValue({ data: order })
    expect(await createOrder(input, "key-1")).toEqual({ ok: true, order })
    const [body, options] = api.storefrontOrdersCreate.mock.calls[0]!
    expect(body).toEqual(input)
    expect(options.headers).toEqual({
      Authorization: "Bearer gsk_test_key",
      "Idempotency-Key": "key-1",
    })
    expect(options.fetchOptions).toEqual({ cache: "no-store" })
  })

  it("passes the API's field messages back for the form (e.g. stock)", async () => {
    const errors = { "lines.0.quantity": ["Not enough stock: 2 available."] }
    api.storefrontOrdersCreate.mockRejectedValue(
      errorWith(422, { message: "invalid", errors })
    )
    expect(await createOrder(input, "k")).toEqual({
      ok: false,
      kind: "invalid",
      errors,
    })
  })

  it("says busy on 429 and unavailable on anything else", async () => {
    api.storefrontOrdersCreate.mockRejectedValueOnce(errorWith(429, {}))
    expect(await createOrder(input, "k")).toEqual({ ok: false, kind: "busy" })
    api.storefrontOrdersCreate.mockRejectedValueOnce(
      errorWith(500, { message: "secret" })
    )
    expect(await createOrder(input, "k")).toEqual({
      ok: false,
      kind: "unavailable",
    })
    expect(JSON.stringify(vi.mocked(console.error).mock.calls)).not.toContain(
      "secret"
    )
  })
})

describe("getOrder", () => {
  it("reads the order fresh, and 404s a malformed id without a call", async () => {
    api.storefrontOrdersGet.mockResolvedValue({ data: order })
    expect(await getOrder(ORDER_ID)).toEqual(order)
    expect(api.storefrontOrdersGet.mock.calls[0]![1].fetchOptions).toEqual({
      cache: "no-store",
    })
    await expect(getOrder("../products")).rejects.toThrow("NEXT_NOT_FOUND")
    expect(api.storefrontOrdersGet).toHaveBeenCalledTimes(1)
  })
})

describe("startPayment", () => {
  it.each([
    { status: "unknown", checkout_url: null },
    { status: "creating", checkout_url: null },
    { status: "open", checkout_url: null },
    { status: "open", checkout_url: "https://pay.example/1", attention: true },
  ])("keeps uncertain payment %j under review", async (payment) => {
    api.storefrontOrdersPayment.mockResolvedValue({ data: { payment_id: "x", ...payment } })
    expect(await startPayment(ORDER_ID, "https://shop.test/orders/1")).toEqual({ ok: false, kind: "pending_review" })
  })
  it("returns where to send the customer, coming back to the order page", async () => {
    api.storefrontOrdersPayment.mockResolvedValue({
      data: {
        payment_id: "x",
        status: "open",
        checkout_url: "https://pay.example/1",
      },
    })
    expect(await startPayment(ORDER_ID, "https://shop.test/orders/1")).toEqual({
      ok: true,
      checkoutUrl: "https://pay.example/1",
    })
    expect(api.storefrontOrdersPayment.mock.calls[0]![1]).toEqual({
      redirect_url: "https://shop.test/orders/1",
    })
  })

  it.each([
    [503, { message: "no payment provider configured" }, "no_provider"],
    [503, { message: "provider unavailable" }, "provider_down"],
    [409, { message: "conflict" }, "not_payable"],
    [500, { message: "secret" }, "unavailable"],
  ])("maps %i %j to %s", async (status, data, kind) => {
    api.storefrontOrdersPayment.mockRejectedValue(errorWith(status, data))
    expect(await startPayment(ORDER_ID, "https://shop.test/o")).toEqual({
      ok: false,
      kind,
    })
  })
})
