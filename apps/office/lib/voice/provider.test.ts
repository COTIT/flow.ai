import { afterEach, expect, it, vi } from "vitest"
import { signedProviderSession } from "./provider"

afterEach(() => {
  vi.unstubAllGlobals()
  vi.unstubAllEnvs()
})

it("requires a provider-allocated conversation ID before exposing signed access", async () => {
  vi.stubEnv("ELEVENLABS_API_KEY", "provider-secret")
  vi.stubEnv("ELEVENLABS_AGENT_ID", "agent-one")
  vi.stubGlobal(
    "fetch",
    vi.fn(async () =>
      Response.json({ signed_url: "wss://api.elevenlabs.io/session" })
    )
  )
  await expect(signedProviderSession()).rejects.toMatchObject({ status: 503 })
})

it("requests a single-use signed session with its server allocation", async () => {
  vi.stubEnv("ELEVENLABS_API_KEY", "provider-secret")
  vi.stubEnv("ELEVENLABS_AGENT_ID", "agent-one")
  const fetcher = vi.fn<(url: string, init: RequestInit) => Promise<Response>>(
    async () =>
      Response.json({
        signed_url: "wss://api.elevenlabs.io/session",
        conversation_id: "conv-one",
      })
  )
  vi.stubGlobal("fetch", fetcher)
  expect(await signedProviderSession()).toEqual({
    signedUrl: "wss://api.elevenlabs.io/session",
    conversationId: "conv-one",
  })
  expect(String(fetcher.mock.calls[0]?.[0])).toContain(
    "include_conversation_id=true"
  )
  expect(fetcher.mock.calls[0]?.[1]).toHaveProperty("redirect", "error")
})

it("extracts allocated conversation identity from the trusted signed URL contract", async () => {
  vi.stubEnv("ELEVENLABS_API_KEY", "provider-secret")
  vi.stubEnv("ELEVENLABS_AGENT_ID", "agent-one")
  vi.stubGlobal(
    "fetch",
    vi.fn(async () =>
      Response.json({
        signed_url:
          "wss://api.elevenlabs.io/session?conversation_id=conv-in-url&conversation_signature=private",
      })
    )
  )
  expect((await signedProviderSession()).conversationId).toBe("conv-in-url")
})
