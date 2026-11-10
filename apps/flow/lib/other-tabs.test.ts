import { beforeEach, describe, expect, it, vi } from "vitest"

// vitest runs in node: an in-memory localStorage and a window that can carry `storage` events.
const saved = new Map<string, string>()
const writes: string[] = []
const storage = {
  getItem: (k: string) => saved.get(k) ?? null,
  setItem: (k: string, v: string) => {
    writes.push(k)
    saved.set(k, v)
  },
  removeItem: (k: string) => {
    writes.push(k)
    saved.delete(k)
  },
}
vi.stubGlobal("localStorage", storage)
const target = new EventTarget()
vi.stubGlobal("window", target)

const KEY = "genesy.session"
const value = (token: string | null, workspaceId: string | null) =>
  JSON.stringify({
    state: { token, workspaceIds: ["w1", "w2"], workspaceId },
    version: 0,
  })

/** What the browser hands this document when another one writes `key`. */
function otherTab(key: string | null, area: unknown = storage) {
  const event = Object.assign(new Event("storage"), {
    key,
    storageArea: area,
  })
  target.dispatchEvent(event)
}

// Fresh modules per test: the store is module state.
async function load() {
  vi.resetModules()
  const { sessionStore } = await import("@genesy/api-client/session")
  const { followOtherTabs } = await import("./other-tabs")
  await sessionStore.persist.rehydrate()
  const cache = { clear: vi.fn() }
  const stop = followOtherTabs(cache)
  writes.length = 0
  return { store: sessionStore, stop, cache }
}

describe("followOtherTabs", () => {
  beforeEach(() => {
    saved.clear()
    saved.set(KEY, value("t1", "w1"))
  })

  it("picks up a workspace switched in another tab, keeping the same login's cache", async () => {
    const { store, stop, cache } = await load()
    saved.set(KEY, value("t1", "w2"))
    otherTab(KEY)
    expect(store.getState()).toMatchObject({ token: "t1", workspaceId: "w2" })
    expect(cache.clear).not.toHaveBeenCalled()
    stop()
  })

  it("picks up another user's login and drops the previous user's cached data", async () => {
    const { store, stop, cache } = await load()
    saved.set(KEY, value("t2", "w1"))
    otherTab(KEY)
    expect(store.getState().token).toBe("t2")
    expect(cache.clear).toHaveBeenCalledOnce()
    stop()
  })

  it("logs out when another tab logged out, and never writes the old token back", async () => {
    const { store, stop } = await load()
    saved.delete(KEY)
    otherTab(KEY)
    expect(store.getState()).toMatchObject({ token: null, workspaceId: null })
    expect(saved.has(KEY)).toBe(false)
    stop()
  })

  it("never writes while following, so a stale view cannot wipe a newer login from another tab", async () => {
    const { store, stop } = await load()
    saved.set(KEY, value("t2", "w2"))
    otherTab(KEY)
    expect(store.getState().token).toBe("t2")
    saved.delete(KEY)
    otherTab(KEY)
    expect(store.getState().token).toBeNull()
    expect(writes).toEqual([])
    stop()
  })

  it("still saves this tab's own changes afterwards", async () => {
    const { store, stop } = await load()
    saved.set(KEY, value("t1", "w2"))
    otherTab(KEY)
    store.getState().selectWorkspace("w1")
    expect(JSON.parse(saved.get(KEY)!).state.workspaceId).toBe("w1")
    stop()
  })

  it("treats a cleared storage (key null) as a logout", async () => {
    const { store, stop } = await load()
    saved.clear()
    otherTab(null)
    expect(store.getState().token).toBeNull()
    expect(saved.has(KEY)).toBe(false)
    stop()
  })

  it("writes nothing when already logged out, so tabs cannot answer each other forever", async () => {
    const { store, stop } = await load()
    saved.delete(KEY)
    otherTab(KEY)
    writes.length = 0
    otherTab(KEY)
    expect(writes).toEqual([])
    expect(store.getState().token).toBeNull()
    stop()
  })

  it("ignores other keys and sessionStorage", async () => {
    const { store, stop } = await load()
    saved.set(KEY, value("t2", "w2"))
    otherTab("genesy.theme")
    otherTab(KEY, { getItem: () => null })
    expect(store.getState()).toMatchObject({ token: "t1", workspaceId: "w1" })
    stop()
  })

  it("stops listening once unsubscribed", async () => {
    const { store, stop } = await load()
    stop()
    saved.delete(KEY)
    otherTab(KEY)
    expect(store.getState().token).toBe("t1")
  })
})
