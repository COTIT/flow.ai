import { afterEach, describe, expect, it, vi } from "vitest"

import { VoiceRequestError } from "./errors"
import { voiceFetch } from "./fetch"

afterEach(() => vi.unstubAllGlobals())

const reply = (status: number, body: unknown) =>
  vi.fn(async () => new Response(JSON.stringify(body), { status }))

describe("voiceFetch", () => {
  it("calls the route under the host's API base and returns the JSON body", async () => {
    const fetch = reply(200, { ok: true })
    vi.stubGlobal("fetch", fetch)
    const expired = vi.fn()
    const { response, value } = await voiceFetch("/office/api/voice", "tools", { method: "POST" }, expired)
    expect(fetch).toHaveBeenCalledWith("/office/api/voice/tools", { method: "POST" })
    expect(response.status).toBe(200)
    expect(value).toEqual({ ok: true })
    expect(expired).not.toHaveBeenCalled()
  })

  it("tells the host when the session expired", async () => {
    vi.stubGlobal("fetch", reply(401, { message: "expired" }))
    const expired = vi.fn()
    const { response } = await voiceFetch("/api/voice", "attachments", {}, expired)
    expect(response.status).toBe(401)
    expect(expired).toHaveBeenCalledOnce()
  })

  it("a dropped request is a network failure, never the browser's TypeError", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => Promise.reject(new TypeError("Failed to fetch"))))
    const failure = await voiceFetch("/api/voice", "tools", {}, vi.fn()).catch((e) => e)
    expect(failure).toBeInstanceOf(VoiceRequestError)
    expect(failure).toMatchObject({ kind: "network", status: 0 })
  })

  it("a dropped request while offline is an offline failure", async () => {
    vi.stubGlobal("navigator", { onLine: false })
    vi.stubGlobal("fetch", vi.fn(async () => Promise.reject(new TypeError("Failed to fetch"))))
    const failure = await voiceFetch("/api/voice", "session", {}, vi.fn()).catch((e) => e)
    expect(failure).toMatchObject({ kind: "offline", status: 0 })
  })

  it("an unreadable 200 body is a bad response", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response("<html>", { status: 200 })))
    const failure = await voiceFetch("/api/voice", "tools", {}, vi.fn()).catch((e) => e)
    expect(failure).toBeInstanceOf(VoiceRequestError)
    expect(failure).toMatchObject({ kind: "bad_response" })
  })

  it("an HTML error page keeps its status for the caller to explain", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response("<html><body>Bad Gateway</body></html>", { status: 502 }))
    )
    const { response, value } = await voiceFetch("/api/voice", "session", {}, vi.fn())
    expect(response.status).toBe(502)
    expect(value).toBeUndefined()
  })

  it("still tells the host about an expired session whose body is not JSON", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response("expired", { status: 401 })))
    const expired = vi.fn()
    const { response } = await voiceFetch("/api/voice", "tools", {}, expired)
    expect(response.status).toBe(401)
    expect(expired).toHaveBeenCalledOnce()
  })
})
