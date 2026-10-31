import { AxiosError } from "axios"
import { beforeEach, describe, expect, it, vi } from "vitest"

vi.stubGlobal("localStorage", {
  getItem: () => null,
  setItem: () => {},
  removeItem: () => {},
})

const login = {
  access_token: "t1",
  token_type: "Bearer",
  workspace_ids: ["w1"],
}

// Fresh modules per test: the client and the store are module state.
async function load(status: number) {
  vi.resetModules()
  const { connectBrowser } = await import("./setup")
  const { sessionStore } = await import("./session")
  connectBrowser().defaults.adapter = async (config) => {
    const response = {
      data: {
        url: config.url,
        baseURL: config.baseURL,
        authorization: config.headers.get("Authorization") ?? null,
      },
      status,
      statusText: "",
      headers: {},
      config,
    }
    if (status >= 400)
      throw new AxiosError("fail", "ERR_BAD_REQUEST", config, null, response)
    return response
  }
  const { apiRequest } = await import("./mutator")
  return { sessionStore, apiRequest }
}

describe("connectBrowser", () => {
  beforeEach(() => vi.resetModules())

  it("calls the same-origin /v1 path with the logged-in token", async () => {
    const { sessionStore, apiRequest } = await load(200)
    sessionStore.getState().signIn(login)

    const body = await apiRequest<{ baseURL: string; authorization: string }>({
      url: "/me",
    })

    expect(body).toMatchObject({ baseURL: "/v1", authorization: "Bearer t1" })
  })

  it("logs out when the backend refuses the token", async () => {
    const { sessionStore, apiRequest } = await load(401)
    sessionStore.getState().signIn(login)

    await expect(apiRequest({ url: "/me" })).rejects.toBeInstanceOf(AxiosError)
    expect(sessionStore.getState().token).toBeNull()
  })
})
