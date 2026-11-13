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
import { conversationHistory, historyTool } from "./history"
import { VoiceError } from "./server"

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

it("creates authorized conversations without a rollout setting", async () => {
  mocks.setting.mockResolvedValue("")
  expect(
    (await history(request({ workspaceId: ws, command: "new" }))).status
  ).toBe(200)
  expect(mocks.ledger).toHaveBeenCalledWith(
    expect.anything(),
    "/conversations",
    "POST",
    {}
  )
})

it("commits signed dialogue before acknowledging webhook, independently of assistant admission", async () => {
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

const changed = "Conversation content changed. Restart pagination."
const modelContext = {
  token: "staff",
  workspaceId: ws,
  sessionId: "synthetic-session",
  apiUrl: "https://api.test/v1",
  permissions: [],
}
const currentPage = {
  messages: [{ role: "user", text: "Use charcoal, rehearsal count 138." }],
  revision: 19,
  through_sequence: "3:3",
  gaps: [],
  has_more: false,
}
function modelReads(outcomes: (Error | typeof currentPage)[]) {
  const messages: string[] = []
  mocks.ledger.mockImplementation(async (_context, suffix: string) => {
    if (suffix.startsWith("/conversations?")) return { conversations: [{ id }] }
    if (suffix.startsWith(`/conversations/${id}/messages`)) {
      messages.push(suffix)
      const result = outcomes.shift()
      if (result instanceof Error) throw result
      return result
    }
    return { conversation: { id, coverage: "complete" }, sources: [] }
  })
  return messages
}

it("restarts a stale model history page once, preserving search and source filters", async () => {
  const reads = modelReads([new VoiceError(changed, 409), currentPage])
  const input = {
    q: "charcoal",
    revision: 15,
    cursor: "stale-page",
    sourceId: id,
    tail: true,
    sourceBefore: 9,
    sourceLimit: 100,
    limit: 100,
  }
  const result = await conversationHistory(modelContext, input)
  expect(reads).toHaveLength(2)
  const before = new URL(`https://api.test${reads[0]}`).searchParams
  const after = new URL(`https://api.test${reads[1]}`).searchParams
  expect(before.get("revision")).toBe("15")
  expect(before.get("cursor")).toBe("stale-page")
  expect(after.has("revision")).toBe(false)
  expect(after.has("cursor")).toBe(false)
  for (const key of ["q", "source_id", "tail", "limit"])
    expect(after.get(key)).toBe(before.get(key))
  expect(input.revision).toBe(15)
  expect(result).toMatchObject({ ...currentPage, pagination_restarted: true })
  expect(result.instruction).toMatch(/restart/i)
  expect(result.instruction).toMatch(/previous pages/i)
  const details = mocks.ledger.mock.calls.filter(
    (call) =>
      !String(call[1]).includes("/messages") &&
      !String(call[1]).startsWith("/conversations?")
  )
  expect(details).toHaveLength(1)
  expect(details[0]?.[1]).toBe(
    `/conversations/${id}?source_before=9&source_limit=100`
  )
})

it("replays the native failed first-read arguments without a stale context revision", async () => {
  const reads = modelReads([
    new VoiceError(changed, 409),
    { ...currentPage, messages: [] },
  ])
  const result = await conversationHistory(modelContext, {
    q: "display preference rehearsal count publishing approved FLOW_CONT_SYNTHETIC",
    revision: 15,
    sourceLimit: 100,
    limit: 100,
  })
  expect(reads).toHaveLength(2)
  const query = new URL(`https://api.test${reads[1]}`).searchParams
  expect(query.has("revision")).toBe(false)
  expect(query.get("q")).toBe(
    "display preference rehearsal count publishing approved FLOW_CONT_SYNTHETIC"
  )
  expect(result).toMatchObject({
    messages: [],
    revision: 19,
    pagination_restarted: true,
  })
})

it("reports incomplete history after a second stale conflict without inventing a snapshot", async () => {
  const reads = modelReads([
    new VoiceError(changed, 409),
    new VoiceError(changed, 409),
  ])
  const result = await conversationHistory(modelContext, { revision: 15 })
  expect(reads).toHaveLength(2)
  expect(result).toMatchObject({
    messages: [],
    sources: [],
    coverage: "partial",
    history_changed: true,
    pagination_restarted: true,
  })
  expect(result).not.toHaveProperty("through_sequence")
  expect(result).not.toHaveProperty("revision")
  expect(result.instruction).toMatch(/not verified/i)
})

it("does not add reads or restart metadata to a successful model history query", async () => {
  const reads = modelReads([currentPage])
  const result = await conversationHistory(modelContext, {
    q: "charcoal",
    revision: 19,
  })
  expect(reads).toHaveLength(1)
  expect(mocks.ledger).toHaveBeenCalledTimes(3)
  expect(result).toMatchObject(currentPage)
  expect(result).not.toHaveProperty("pagination_restarted")
})

it.each([
  new VoiceError(changed, 401),
  new VoiceError("Access denied.", 403),
  new VoiceError("Conversation deleted.", 404),
  new VoiceError("Session expired.", 410),
  new VoiceError("This assistant session has ended.", 409),
  new Error(changed),
])("never restarts another history failure: %s", async (error) => {
  const reads = modelReads([error])
  await expect(
    conversationHistory(modelContext, { revision: 15 })
  ).rejects.toBe(error)
  expect(reads).toHaveLength(1)
})

it("preserves a different failure from the one fresh history read", async () => {
  const denied = new VoiceError("Access denied.", 403)
  const reads = modelReads([new VoiceError(changed, 409), denied])
  await expect(
    conversationHistory(modelContext, { revision: 15 })
  ).rejects.toBe(denied)
  expect(reads).toHaveLength(2)
})

it("keeps stale browser history pages rejected without a fresh-read retry", async () => {
  mocks.ledger.mockRejectedValue(new VoiceError(changed, 409))
  const response = await history(
    request({
      workspaceId: ws,
      command: "messages",
      logicalConversationId: id,
      revision: 15,
      cursor: "stale-page",
    })
  )
  expect(response.status).toBe(409)
  expect(await response.json()).toMatchObject({ message: changed })
  expect(mocks.ledger).toHaveBeenCalledTimes(1)
})

it("describes fresh reads, returned page revisions and literal substring searches", () => {
  expect(historyTool.summary).toMatch(/first read.*omit.*revision.*cursor/i)
  expect(historyTool.body.properties.q).toMatchObject({
    description: expect.stringMatching(/case-insensitive literal substring/i),
  })
  expect(historyTool.body.properties.revision).toMatchObject({
    description: expect.stringMatching(/prior.*page/i),
  })
})
