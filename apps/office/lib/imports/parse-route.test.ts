import { afterEach, describe, expect, it, vi } from "vitest"
import { POST } from "@/app/api/imports/parse/route"

const WS = "11111111-1111-1111-1111-111111111111"

function request(
  file: Blob | null,
  headers: Record<string, string> = {
    authorization: "Bearer token",
    "x-workspace-id": WS,
  }
) {
  const form = new FormData()
  if (file) form.append("file", file, "list.csv")
  return new Request("http://office.test/api/imports/parse", {
    method: "POST",
    body: form,
    headers,
  })
}

function member(workspaces: string[] = [WS], status = 200) {
  const fetch = vi.fn(async () =>
    Response.json({ data: workspaces.map((id) => ({ id })) }, { status })
  )
  vi.stubGlobal("fetch", fetch)
  return fetch
}

afterEach(() => vi.unstubAllGlobals())

describe("POST /api/imports/parse", () => {
  it("parses a csv for a workspace member and writes nothing", async () => {
    const fetch = member()
    const res = await POST(
      request(new Blob(["name,price\nRice,2.50\n"], { type: "text/csv" }))
    )
    expect(res.status).toBe(200)
    expect(await res.json()).toEqual({
      data: {
        columns: ["name", "price"],
        rows: [{ line: 2, cells: ["Rice", "2.50"] }],
      },
    })
    expect(fetch).toHaveBeenCalledTimes(1)
    const [url, init] = fetch.mock.calls[0] as unknown as [string, RequestInit]
    expect(url).toMatch(/\/me\/workspaces$/)
    expect(init.method ?? "GET").toBe("GET")
  })

  it("needs a bearer: 401", async () => {
    const fetch = member()
    const res = await POST(
      request(new Blob(["a\nb"]), { "x-workspace-id": WS })
    )
    expect(res.status).toBe(401)
    expect(fetch).not.toHaveBeenCalled()
  })

  it("refuses an expired login (401) and a foreign workspace (403)", async () => {
    member([], 401)
    expect((await POST(request(new Blob(["a\nb"])))).status).toBe(401)
    member(["22222222-2222-2222-2222-222222222222"])
    const res = await POST(request(new Blob(["a\nb"])))
    expect(res.status).toBe(403)
    expect((await res.json()).message).toBe(
      "You do not have access to this workspace."
    )
  })

  it("refuses files above 2 MiB (413), old Excel (415) and a missing file (422)", async () => {
    member()
    const big = new Blob([new Uint8Array(2 * 1024 * 1024 + 1).fill(97)])
    expect((await POST(request(big))).status).toBe(413)
    const ole = new Blob([new Uint8Array([0xd0, 0xcf, 0x11, 0xe0])])
    expect((await POST(request(ole))).status).toBe(415)
    expect((await POST(request(null))).status).toBe(422)
  })
})
