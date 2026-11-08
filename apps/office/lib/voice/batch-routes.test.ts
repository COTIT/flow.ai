import { beforeEach, expect, it, vi } from "vitest"
const mocks = vi.hoisted(() => ({
  authorize: vi.fn(),
  session: vi.fn(),
  ledger: vi.fn(),
  propose: vi.fn(),
  restore: vi.fn(),
  spans: [] as string[],
  modes: [] as string[],
}))
vi.mock("./identity", () => ({
  authorizeVoiceSession: async (
    request: Request,
    workspaceId: unknown,
    sessionId: unknown,
    mode: string
  ) => {
    mocks.modes.push(mode)
    const context = await mocks.authorize(request, workspaceId)
    const session = await mocks.session(context, sessionId)
    return {
      context: {
        ...context,
        sessionId: session.id,
        language: session.language ?? "en",
      },
      session,
    }
  },
}))
vi.mock("./tracing", () => ({
  noteVoiceFailure: () => {},
  withVoiceTracing: async (name: string, action: () => Promise<unknown>) => {
    mocks.spans.push(name)
    return action()
  },
}))
vi.mock("./server", async (original) => ({
  ...(await original<typeof import("./server")>()),
  authorizeVoice: mocks.authorize,
}))
vi.mock("./bridge", async (original) => ({
  ...(await original<typeof import("./bridge")>()),
  requireAssistantSession: mocks.session,
  readAssistantSession: mocks.session,
  assistantRequest: mocks.ledger,
}))
vi.mock("./batches", async (original) => ({
  ...(await original<typeof import("./batches")>()),
  proposeBatch: mocks.propose,
  restoreBatch: mocks.restore,
}))
import { VoiceError } from "./server"
import { POST } from "@/app/api/voice/batch/route"
const ctx = {
  token: "staff",
  workspaceId: "01950000-0000-7000-8000-000000000009",
  permissions: ["workspace_admin"],
  apiUrl: "https://api.test/v1",
  sessionId: "s",
}
const request = (body: unknown) =>
  new Request("http://office.test/api/voice/batch", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  })
beforeEach(() => {
  vi.clearAllMocks()
  mocks.spans.length = 0
  mocks.modes.length = 0
  vi.stubEnv("ASSISTANT_ENABLED", "true")
  vi.stubEnv("ASSISTANT_WORKSPACE_ALLOWLIST", ctx.workspaceId)
  mocks.authorize.mockResolvedValue({ ...ctx })
  mocks.session.mockResolvedValue({
    id: "s",
    state: "active",
    conversation_id: "conv",
    language: "en",
  })
  mocks.propose.mockResolvedValue({
    status: "requires_confirmation",
    data: { batchId: "batch" },
  })
  mocks.ledger.mockResolvedValue({
    id: "batch",
    session_id: "s",
    state: "paused",
    steps: [],
    next_index: 0,
    action_count: 2,
    expires_at: new Date(Date.now() + 120000).toISOString(),
  })
})
it("prepares a private canonical batch after normal owner/session admission", async () => {
  const calls = [
    { operationId: "warehouses_create", arguments: { body: { name: "A" } } },
    { operationId: "warehouses_create", arguments: { body: { name: "B" } } },
  ]
  const response = await POST(
    request({ command: "prepare", workspaceId: "w", sessionId: "s", calls })
  )
  expect(response.status).toBe(200)
  expect((await response.json()).status).toBe("requires_confirmation")
  expect(mocks.propose).toHaveBeenCalledWith(
    calls,
    expect.objectContaining(ctx),
    expect.any(Map)
  )
})
it("adds model-reported unclear lines to the batch card only", async () => {
  mocks.propose.mockResolvedValue({
    status: "requires_confirmation",
    review: { details: {} },
  })
  const calls = [{ operationId: "warehouses_create", arguments: {} }]
  const response = await POST(
    request({
      command: "prepare",
      workspaceId: "w",
      sessionId: "s",
      calls,
      unclear: [{ seen: "Beans ?? bags" }],
    })
  )
  expect((await response.json()).review.details.notIncluded).toEqual([
    { text: "Beans ?? bags", reason: "unclear" },
  ])
  expect(mocks.propose.mock.calls[0]![0]).toEqual(calls)
})
it.each(["approve", "start", "advance"])(
  "never exposes model-side %s authority",
  async (command) => {
    const response = await POST(
      request({ command, workspaceId: "w", sessionId: "s", batchId: "batch" })
    )
    expect(response.status).toBe(400)
    expect(mocks.ledger).not.toHaveBeenCalled()
  }
)
it("keeps cancellation available while assistant admissions are disabled", async () => {
  vi.stubEnv("ASSISTANT_ENABLED", "false")
  const response = await POST(
    request({
      command: "pause",
      workspaceId: "w",
      sessionId: "s",
      batchId: "01950000-0000-7000-8000-000000000001",
      reason: "disconnected",
    })
  )
  expect(response.status).toBe(200)
  expect(mocks.ledger).toHaveBeenCalledWith(
    expect.anything(),
    expect.stringContaining("/pause"),
    "POST",
    expect.objectContaining({ session_id: "s", reason: "disconnected" })
  )
})

it("localizes stale batch errors using owned session language", async () => {
  mocks.session.mockResolvedValue({
    id: "s",
    state: "active",
    conversation_id: "conv",
    language: "nl",
  })
  mocks.propose.mockRejectedValue(
    new VoiceError("Fixed batch membership changed.", 409)
  )
  const response = await POST(
    request({ command: "prepare", workspaceId: "w", sessionId: "s", calls: [] })
  )
  expect(response.status).toBe(409)
  expect((await response.json()).message).toBe(
    "Deze reeks is gewijzigd, gestopt of verlopen. Controleer de vastgelegde stappen en beoordeel een nieuwe reeks."
  )
})

it("reads batch lists inside one request span without requiring a live session", async () => {
  await POST(
    request({ command: "list", workspaceId: ctx.workspaceId, sessionId: "s" })
  )
  expect(mocks.spans).toEqual(["genesy_assistant_batch"])
  expect(mocks.modes).toEqual(["read"])
})
