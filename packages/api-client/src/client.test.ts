import { AxiosError, type AxiosAdapter } from "axios"
import { describe, expect, it } from "vitest"

import { createApiClient } from "./client"

// Echo adapter: returns the outgoing request instead of calling the network.
const echo: AxiosAdapter = async (config) => ({
  data: {
    url: `${config.baseURL}${config.url}`,
    authorization: config.headers.get("Authorization") ?? null,
    accept: config.headers.get("Accept") ?? null,
  },
  status: 200,
  statusText: "OK",
  headers: {},
  config,
})

describe("createApiClient", () => {
  it("sends requests to the given base URL", async () => {
    const api = createApiClient({ baseURL: "http://api.test/v1" })
    api.defaults.adapter = echo

    const { data } = await api.get("/products")

    expect(data.url).toBe("http://api.test/v1/products")
  })

  it("asks for JSON", async () => {
    const api = createApiClient({ baseURL: "http://api.test" })
    api.defaults.adapter = echo

    const { data } = await api.get("/x")

    expect(data.accept).toBe("application/json")
  })

  it("attaches the bearer token when one is available", async () => {
    const api = createApiClient({
      baseURL: "http://api.test",
      getToken: () => "abc123",
    })
    api.defaults.adapter = echo

    const { data } = await api.get("/me")

    expect(data.authorization).toBe("Bearer abc123")
  })

  it("sends no Authorization header when logged out", async () => {
    const api = createApiClient({
      baseURL: "http://api.test",
      getToken: () => null,
    })
    api.defaults.adapter = echo

    const { data } = await api.get("/me")

    expect(data.authorization).toBeNull()
  })

  it("reads the token on every request, so a new login applies at once", async () => {
    let token: string | null = null
    const api = createApiClient({
      baseURL: "http://api.test",
      getToken: () => token,
    })
    api.defaults.adapter = echo

    const before = await api.get("/me")
    token = "fresh"
    const after = await api.get("/me")

    expect(before.data.authorization).toBeNull()
    expect(after.data.authorization).toBe("Bearer fresh")
  })

  it("tells the app when the backend says the token is no longer valid", async () => {
    let calls = 0
    const api = createApiClient({
      baseURL: "http://api.test",
      onUnauthorized: () => calls++,
    })
    // Real adapters reject non-2xx answers; this one does the same.
    api.defaults.adapter = async (config) => {
      const response = {
        data: {},
        status: 401,
        statusText: "",
        headers: {},
        config,
      }
      throw new AxiosError(
        "Unauthorized",
        "ERR_BAD_REQUEST",
        config,
        null,
        response
      )
    }

    await expect(api.get("/me")).rejects.toMatchObject({
      response: { status: 401 },
    })
    expect(calls).toBe(1)
  })
})
