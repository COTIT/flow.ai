import { createHash, createHmac } from "node:crypto"
import { afterEach, expect, it, vi } from "vitest"
import {
  assistantRequest,
  bridgeHeaders,
  readAssistantSession,
  readBounded,
  requireAssistantSession,
} from "./bridge"

afterEach(() => {
  vi.unstubAllGlobals()
  vi.unstubAllEnvs()
})

it("signs exact backend path and bytes without retaining bearer in signature", async () => {
  vi.stubEnv("ASSISTANT_BRIDGE_SECRET", "x".repeat(32))
  const body = '{"session_id":"s1"}'
  const path = "/v1/workspace/w1/assistant/actions/a1/approve"
  const headers = await bridgeHeaders("POST", path, body, 123)
  const expected = createHmac("sha256", "x".repeat(32))
    .update(
      `123\nPOST\n${path}\n${createHash("sha256").update(body).digest("hex")}`
    )
    .digest("hex")
  expect(headers["Assistant-Bridge-Signature"]).toBe(expected)
  expect(headers["Assistant-Bridge-Timestamp"]).toBe("123")
})

it("fails closed when server signing configuration is absent", async () => {
  vi.stubEnv("ASSISTANT_BRIDGE_SECRET", "")
  vi.stubEnv("ASSISTANT_BRIDGE_SECRET_FILE", "")
  await expect(bridgeHeaders("POST", "/v1/a", "")).rejects.toMatchObject({
    status: 503,
  })
})

it("uses per-request identity and refuses redirects on assistant API calls", async () => {
  vi.stubEnv("ASSISTANT_BRIDGE_SECRET", "x".repeat(32))
  const fetcher = vi.fn<
    (url: string | URL, options: RequestInit) => Promise<Response>
  >(async () => Response.json({ data: { id: "a" } }))
  vi.stubGlobal("fetch", fetcher)
  const ctx = {
    token: "one",
    workspaceId: "w1",
    apiUrl: "https://backend.test/v1",
    permissions: [],
  }
  await assistantRequest(
    ctx,
    "/sessions/s1/attach",
    "POST",
    { conversation_id: "conv_1" },
    true
  )
  expect(String(fetcher.mock.calls[0]?.[0])).toBe(
    "https://backend.test/v1/workspace/w1/assistant/sessions/s1/attach"
  )
  expect(fetcher.mock.calls[0]?.[1]).toMatchObject({
    redirect: "error",
    headers: { Authorization: "Bearer one" },
  })
})

it("counts streamed bytes instead of trusting Content-Length", async () => {
  const response = new Response(
    new ReadableStream({
      start(c) {
        c.enqueue(new Uint8Array(3))
        c.enqueue(new Uint8Array(3))
        c.close()
      },
    })
  )
  await expect(readBounded(response.body, 5)).rejects.toMatchObject({
    status: 413,
  })
})

it("cancels a stalled input instead of holding binary permits indefinitely", async () => {
  let cancelled = false
  const body = new ReadableStream<Uint8Array>({
    cancel() {
      cancelled = true
    },
  })
  await expect(readBounded(body, 10, 10)).rejects.toMatchObject({ status: 408 })
  expect(cancelled).toBe(true)
})

it("allows owned ended-session history without reactivating provider access", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () =>
      Response.json({
        data: {
          id: "s1",
          state: "closed",
          closed_at: new Date().toISOString(),
          expires_at: new Date(0).toISOString(),
        },
      })
    )
  )
  const ctx = {
    token: "one",
    workspaceId: "w1",
    apiUrl: "https://backend.test/v1",
    permissions: [],
  }
  expect((await readAssistantSession(ctx, "s1")).state).toBe("closed")
  await expect(requireAssistantSession(ctx, "s1")).rejects.toMatchObject({
    status: 410,
  })
})
