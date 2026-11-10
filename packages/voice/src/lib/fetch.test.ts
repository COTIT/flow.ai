import { afterEach, describe, expect, it, vi } from "vitest"

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
})
