import type { AxiosAdapter } from "axios"
import { beforeEach, describe, expect, it, vi } from "vitest"

// Echo adapter: returns the outgoing request instead of calling the network.
const echo: AxiosAdapter = async (config) => ({
  data: {
    url: `${config.baseURL}${config.url}`,
    method: config.method,
    params: config.params ?? null,
    authorization: config.headers.get("Authorization") ?? null,
    contentType: config.headers.get("Content-Type") ?? null,
    idempotencyKey: config.headers.get("Idempotency-Key") ?? null,
    hasSignal: Boolean(config.signal),
  },
  status: 200,
  statusText: "OK",
  headers: {},
  config,
})

// Fresh module per test: the configured client is module state.
async function load() {
  vi.resetModules()
  return import("./mutator")
}

describe("apiRequest", () => {
  beforeEach(() => vi.resetModules())

  it("refuses to send before the app configured the backend", async () => {
    const { apiRequest } = await load()
    await expect(apiRequest({ url: "/x", method: "GET" })).rejects.toThrow(
      "configureApi"
    )
  })

  it("sends to the configured backend and returns only the body", async () => {
    const { apiRequest, configureApi } = await load()
    configureApi({ baseURL: "http://api.test/v1" }).defaults.adapter = echo

    const body = await apiRequest<{ url: string; method: string }>({
      url: "/workspace/ws-1/currencies",
      method: "GET",
    })

    expect(body.url).toBe("http://api.test/v1/workspace/ws-1/currencies")
    expect(body.method).toBe("get")
  })

  it("adds the login token from the app", async () => {
    const { apiRequest, configureApi } = await load()
    configureApi({
      baseURL: "http://api.test",
      getToken: () => "tok",
    }).defaults.adapter = echo

    const body = await apiRequest<{ authorization: string }>({ url: "/me" })

    expect(body.authorization).toBe("Bearer tok")
  })

  it("adds per-call headers without dropping the generated ones", async () => {
    const { apiRequest, configureApi } = await load()
    configureApi({ baseURL: "http://api.test" }).defaults.adapter = echo

    const body = await apiRequest<{
      contentType: string
      idempotencyKey: string
    }>(
      {
        url: "/x",
        method: "POST",
        headers: { "Content-Type": "application/json" },
        data: '{"a":1}',
      },
      { headers: { "Idempotency-Key": "k-1" } }
    )

    expect(body.contentType).toBe("application/json")
    expect(body.idempotencyKey).toBe("k-1")
  })

  it("merges per-call options such as a cancel signal", async () => {
    const { apiRequest, configureApi } = await load()
    configureApi({ baseURL: "http://api.test" }).defaults.adapter = echo

    const body = await apiRequest<{ hasSignal: boolean; params: unknown }>(
      { url: "/x", params: { q: "eur" } },
      { signal: new AbortController().signal }
    )

    expect(body.hasSignal).toBe(true)
    expect(body.params).toEqual({ q: "eur" })
  })
})
