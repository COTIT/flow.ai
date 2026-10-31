import type { AxiosAdapter } from "axios"
import { beforeEach, describe, expect, it, vi } from "vitest"

// Echo adapter: returns the outgoing request instead of calling the network.
const echo: AxiosAdapter = async (config) => ({
  data: {
    url: `${config.baseURL}${config.url}`,
    params: config.params ?? null,
    authorization: config.headers.get("Authorization") ?? null,
  },
  status: 200,
  statusText: "OK",
  headers: {},
  config,
})

// Fresh modules per test: the configured client is module state.
async function load() {
  vi.resetModules()
  const { configureApi } = await import("./mutator")
  configureApi({ baseURL: "http://api.test/v1" }).defaults.adapter = echo
  return import("./pending")
}

describe("pending endpoints", () => {
  beforeEach(() => vi.resetModules())

  it("lists my workspaces", async () => {
    const { meWorkspaces } = await load()
    const body = (await meWorkspaces()) as unknown as { url: string }
    expect(body.url).toBe("http://api.test/v1/me/workspaces")
  })

  it("lists one workspace's products, page by page", async () => {
    const { productsList } = await load()
    const body = (await productsList("ws-1", { page: 2 })) as unknown as {
      url: string
      params: unknown
    }
    expect(body.url).toBe("http://api.test/v1/workspace/ws-1/products")
    expect(body.params).toEqual({ page: 2 })
  })

  it("lets server code pass the token per call instead of the shared store", async () => {
    const { meWorkspaces } = await load()
    const body = (await meWorkspaces({ headers: { Authorization: "Bearer srv" } })) as unknown as {
      authorization: string
    }
    expect(body.authorization).toBe("Bearer srv")
  })
})
