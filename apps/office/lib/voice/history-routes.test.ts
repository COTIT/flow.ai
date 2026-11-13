import { createHmac } from "node:crypto"
import { afterEach, beforeEach, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({
  authorize: vi.fn(),
  ledger: vi.fn(),
  setting: vi.fn(),
  fetch: vi.fn(),
}))
vi.mock("./server", async (original) => ({
  ...(await original<typeof import("./server")>()),
  authorizeVoice: mocks.authorize,
}))
vi.mock("./bridge", async (original) => ({
  ...(await original<typeof import("./bridge")>()),
  assistantRequest: mocks.ledger,
}))
vi.mock("./settings", () => ({ voiceSetting: mocks.setting }))

import { POST as history } from "@/app/api/voice/history/route"
import { POST as webhook } from "@/app/api/voice/history-webhook/route"

const ws = "01950000-0000-7000-8000-000000000001"
const id = "01950000-0000-7000-8000-000000000002"
const secret = "synthetic-history-webhook-secret-only"
beforeEach(() => {
  mocks.authorize.mockReset().mockResolvedValue({
    token: "staff",
    workspaceId: ws,
    userId: "user",
    apiUrl: "https://api.test/v1",
    permissions: [],
  })
  mocks.ledger.mockReset().mockResolvedValue({ conversations: [] })
  mocks.setting.mockReset().mockImplementation(
    async (key: string) =>
      ({
        ASSISTANT_CONVERSATIONS_ENABLED: "true",
        ELEVENLABS_WEBHOOK_SECRET: secret,
        ELEVENLABS_AGENT_ID: "agent_test",
        ASSISTANT_BRIDGE_SECRET: secret,
        API_URL: "https://api.test/v1",
      })[key] ?? ""
  )
  mocks.fetch
    .mockReset()
    .mockResolvedValue(Response.json({ data: { status: "saved" } }))
  vi.stubGlobal("fetch", mocks.fetch)
})
afterEach(() => vi.unstubAllGlobals())
const request = (body: unknown) =>
  new Request("https://office.test/api/voice/history", {
    method: "POST",
    headers: {
      authorization: "Bearer staff",
      "content-type": "application/json",
    },
    body: JSON.stringify(body),
  })

it("authorizes history reads and forwards only bounded supported query fields", async () => {
  const result = await history(
    request({
      workspaceId: ws,
      command: "messages",
      logicalConversationId: id,
      q: "red header",
      tail: true,
    })
  )
  expect(result.status).toBe(200)
  expect(mocks.authorize).toHaveBeenCalled()
  expect(mocks.ledger.mock.calls[0]?.[1]).toContain(
    `/conversations/${id}/messages?`
  )
  expect(mocks.ledger.mock.calls[0]?.[1]).toContain("q=red+header")
  expect(mocks.ledger.mock.calls[0]?.[1]).toContain("tail=true")
  expect(mocks.ledger.mock.calls[0]?.[1]).not.toContain("staff")
})

it("refuses arbitrary commands, paths and credentials before storage calls", async () => {
  for (const body of [
    { command: "execute", logicalConversationId: id },
    { command: "delete", logicalConversationId: "../foreign" },
    { command: "messages", logicalConversationId: id, limit: 101 },
  ]) {
    const response = await history(request({ workspaceId: ws, ...body }))
    expect(response.status).toBe(400)
  }
  expect(mocks.ledger).not.toHaveBeenCalled()
})

it("keeps new conversation creation off unless explicitly enabled", async () => {
  mocks.setting.mockResolvedValue("")
  expect(
    (await history(request({ workspaceId: ws, command: "new" }))).status
  ).toBe(503)
  expect(mocks.ledger).not.toHaveBeenCalled()
})

it("commits signed dialogue before acknowledging webhook, even when activation is off", async () => {
  const payload = JSON.stringify({
    type: "post_call_transcription",
    data: {
      conversation_id: "conv_test",
      agent_id: "agent_test",
      status: "done",
      transcript: [{ role: "user", message: "Use red" }],
    },
  })
  const timestamp = Math.floor(Date.now() / 1000)
  const digest = createHmac("sha256", secret)
    .update(`${timestamp}.${payload}`)
    .digest("hex")
  const req = () =>
    new Request("https://office.test/api/voice/history-webhook", {
      method: "POST",
      body: payload,
      headers: {
        "content-type": "application/json",
        "elevenlabs-signature": `t=${timestamp},v0=${digest}`,
      },
    })
  const response = await webhook(req())
  expect(response.status).toBe(200)
  const [url, options] = mocks.fetch.mock.calls[0]!
  expect(String(url)).toBe("https://api.test/v1/assistant/conversations/ingest")
  expect(JSON.parse(options.body)).toMatchObject({
    provider_conversation_id: "conv_test",
    turns: [{ text: "Use red" }],
  })
  expect(options.headers["Assistant-Bridge-Signature"]).toBeTruthy()
  mocks.fetch.mockRejectedValueOnce(new Error("private database detail"))
  const failed = await webhook(req())
  expect(failed.status).toBe(503)
  expect(await failed.text()).not.toContain("private database detail")
})

it("rejects bad provider signatures before ingest", async () => {
  const response = await webhook(
    new Request("https://office.test/api/voice/history-webhook", {
      method: "POST",
      body: "{}",
      headers: {
        "content-type": "application/json",
        "elevenlabs-signature": "invalid",
      },
    })
  )
  expect(response.status).toBe(401)
  expect(mocks.fetch).not.toHaveBeenCalled()
})
