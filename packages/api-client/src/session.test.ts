import { beforeEach, describe, expect, it, vi } from "vitest"

// In-memory localStorage: vitest runs in node, which has none.
const saved = new Map<string, string>()
vi.stubGlobal("localStorage", {
  getItem: (k: string) => saved.get(k) ?? null,
  setItem: (k: string, v: string) => void saved.set(k, v),
  removeItem: (k: string) => void saved.delete(k),
})

const login = { access_token: "t1", token_type: "Bearer", workspace_ids: ["w1", "w2"] }

// Fresh module per test: the store is module state.
async function load() {
  vi.resetModules()
  const { sessionStore } = await import("./session")
  await sessionStore.persist.rehydrate()
  return sessionStore
}

describe("sessionStore", () => {
  beforeEach(() => saved.clear())

  it("starts logged out", async () => {
    const store = await load()
    expect(store.getState()).toMatchObject({ token: null, workspaceId: null, workspaceIds: [] })
  })

  it("keeps the token and opens the first workspace on login", async () => {
    const store = await load()
    store.getState().signIn(login)
    expect(store.getState()).toMatchObject({ token: "t1", workspaceIds: ["w1", "w2"], workspaceId: "w1" })
  })

  it("switches only to a workspace the login belongs to", async () => {
    const store = await load()
    store.getState().signIn(login)

    store.getState().selectWorkspace("w2")
    expect(store.getState().workspaceId).toBe("w2")

    store.getState().selectWorkspace("someone-elses")
    expect(store.getState().workspaceId).toBe("w2")
  })

  it("forgets everything on logout", async () => {
    const store = await load()
    store.getState().signIn(login)
    store.getState().signOut()
    expect(store.getState()).toMatchObject({ token: null, workspaceId: null, workspaceIds: [] })
  })

  it("survives a page reload", async () => {
    ;(await load()).getState().signIn(login)
    const reloaded = await load()
    expect(reloaded.getState()).toMatchObject({ token: "t1", workspaceId: "w1", hydrated: true })
  })
})
