import { beforeEach, describe, expect, it, vi } from "vitest"

import { serviceWorkerSource, webManifest } from "./pwa"

describe("webManifest", () => {
  const manifest = webManifest({
    name: "Genesy Till",
    shortName: "Till",
    startUrl: "/sale",
    themeColor: "#1a1a19",
    backgroundColor: "#faf8f3",
  })

  it("opens full screen like an app, at the given start page", () => {
    expect(manifest).toMatchObject({
      id: "/",
      name: "Genesy Till",
      short_name: "Till",
      start_url: "/sale",
      scope: "/",
      display: "standalone",
      theme_color: "#1a1a19",
      background_color: "#faf8f3",
    })
  })

  it("lists the icon sizes Android needs to offer an install, plus a maskable one", () => {
    expect(manifest.icons).toEqual([
      { src: "/icons/icon-192.png", sizes: "192x192", type: "image/png" },
      { src: "/icons/icon-512.png", sizes: "512x512", type: "image/png" },
      {
        src: "/icons/icon-maskable-512.png",
        sizes: "512x512",
        type: "image/png",
        purpose: "maskable",
      },
    ])
  })
})

/** Runs the real service worker text against an in-memory Cache API and a fake network. */
function boot(precache: string[], version = "1") {
  const stores = new Map<string, Map<string, Response>>()
  const cacheOf = (name: string) => {
    if (!stores.has(name)) stores.set(name, new Map())
    const m = stores.get(name)!
    return {
      match: async (k: string) => m.get(k)?.clone(),
      put: async (k: string, r: Response) => void m.set(k, r),
    }
  }
  const caches = {
    open: async (name: string) => cacheOf(name),
    keys: async () => [...stores.keys()],
    delete: async (name: string) => stores.delete(name),
  }
  const listeners: Record<string, (event: unknown) => void> = {}
  const self = {
    location: { origin: "https://app.test" },
    addEventListener: (type: string, fn: (event: unknown) => void) =>
      (listeners[type] = fn),
    skipWaiting: async () => undefined,
    clients: { claim: async () => undefined },
  }
  let online = true
  const network = new Map<string, string>()
  const fetch = vi.fn(async (input: string | { url: string }) => {
    if (!online) throw new TypeError("Failed to fetch")
    const url = new URL(typeof input === "string" ? input : input.url, self.location.origin)
    const body = network.get(url.pathname)
    return body === undefined
      ? new Response("missing", { status: 404 })
      : new Response(body, { status: 200 })
  })
  new Function("self", "caches", "fetch", serviceWorkerSource({ precache, version }))(
    self,
    caches,
    fetch
  )

  const lifecycle = async (type: "install" | "activate") => {
    let done: Promise<unknown> = Promise.resolve()
    listeners[type]!({ waitUntil: (p: Promise<unknown>) => (done = p) })
    await done
  }
  /** The response the worker gives, or null when it leaves the request to the browser. */
  const request = async (
    path: string,
    init: { method?: string; mode?: string; headers?: Record<string, string> } = {}
  ) => {
    let answer: Promise<Response> | null = null
    listeners.fetch!({
      request: {
        url: new URL(path, self.location.origin).href,
        method: init.method ?? "GET",
        mode: init.mode ?? "cors",
        headers: new Headers(init.headers),
      },
      respondWith: (p: Promise<Response>) => (answer = p),
    })
    return answer ? await (answer as Promise<Response>) : null
  }
  return {
    stores,
    network,
    fetch,
    lifecycle,
    request,
    offline: () => (online = false),
  }
}

describe("serviceWorkerSource", () => {
  let sw: ReturnType<typeof boot>
  beforeEach(() => {
    sw = boot(["/offline", "/sale"])
    sw.network.set("/offline", "<p>offline</p>")
    sw.network.set(
      "/sale",
      // A script tag, plus a chunk named only inside Next's escaped RSC payload.
      '<script src="/_next/static/chunks/a.js"></script><script>self.__next_f.push([1,"\\"/_next/static/chunks/b.js\\""])</script>'
    )
    sw.network.set("/_next/static/chunks/a.js", "a")
    sw.network.set("/_next/static/chunks/b.js", "b")
  })

  it("never answers API calls, writes or other sites: those always reach the network", async () => {
    await sw.lifecycle("install")
    expect(await sw.request("/v1/me")).toBeNull()
    expect(await sw.request("/v1/till/sales", { method: "POST" })).toBeNull()
    expect(await sw.request("/sale", { method: "POST", mode: "navigate" })).toBeNull()
    expect(await sw.request("https://other.test/x.js")).toBeNull()
    const kept = [...sw.stores.values()].flatMap((m) => [...m.keys()])
    expect(kept.some((k) => k.includes("/v1/"))).toBe(false)
  })

  it("keeps the listed pages and every build file they load, so the app starts with no network", async () => {
    await sw.lifecycle("install")
    sw.offline()
    expect(await (await sw.request("/sale", { mode: "navigate" }))!.text()).toContain("a.js")
    expect(await (await sw.request("/_next/static/chunks/a.js"))!.text()).toBe("a")
    expect(await (await sw.request("/_next/static/chunks/b.js"))!.text()).toBe("b")
  })

  it("shows the newest page when online, the kept copy when offline, else the offline page", async () => {
    await sw.lifecycle("install")
    sw.network.set("/orders", "fresh orders")
    expect(await (await sw.request("/orders?page=2", { mode: "navigate" }))!.text()).toBe(
      "fresh orders"
    )
    sw.offline()
    expect(await (await sw.request("/orders", { mode: "navigate" }))!.text()).toBe(
      "fresh orders"
    )
    expect(await (await sw.request("/never-seen", { mode: "navigate" }))!.text()).toBe(
      "<p>offline</p>"
    )
  })

  it("answers Next's in-app page fetch offline with the kept page (Next then loads it in full), and never keeps those fetches", async () => {
    await sw.lifecycle("install")
    sw.network.set("/queue", "flight data")
    const rsc = { headers: { RSC: "1" } }
    expect(await (await sw.request("/queue?_rsc=abc", rsc))!.text()).toBe("flight data")
    sw.offline()
    expect(await (await sw.request("/sale?_rsc=abc", rsc))!.text()).toContain("a.js")
    expect(await (await sw.request("/queue?_rsc=abc", rsc))!.text()).toBe("<p>offline</p>")
  })

  it("serves build files from the cache once fetched (their names change with every build)", async () => {
    sw.network.set("/_next/static/css/x.css", "css")
    await sw.request("/_next/static/css/x.css")
    sw.offline()
    expect(await (await sw.request("/_next/static/css/x.css"))!.text()).toBe("css")
  })

  it("does not keep error pages", async () => {
    await sw.lifecycle("install")
    await sw.request("/gone", { mode: "navigate" })
    sw.offline()
    expect(await (await sw.request("/gone", { mode: "navigate" }))!.text()).toBe(
      "<p>offline</p>"
    )
  })

  it("fails the install when a listed page cannot be fetched, so a broken shell never replaces a good one", async () => {
    const broken = boot(["/missing"])
    await expect(broken.lifecycle("install")).rejects.toThrow("/missing")
  })

  it("drops pages from older versions on update but keeps build files", async () => {
    const old = boot(["/offline"], "1")
    old.network.set("/offline", "old")
    old.network.set("/_next/static/chunks/a.js", "a")
    await old.lifecycle("install")
    await old.request("/_next/static/chunks/a.js")
    const fresh = boot(["/offline"], "2")
    for (const [name, store] of old.stores) fresh.stores.set(name, store)
    fresh.network.set("/offline", "new")
    await fresh.lifecycle("install")
    await fresh.lifecycle("activate")
    expect([...fresh.stores.keys()].sort()).toEqual(["pages-2", "static"])
  })
})
