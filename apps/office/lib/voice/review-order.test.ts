import { beforeEach, expect, it, vi } from "vitest"
import type { PreparedAction } from "./catalog"

// Real review workflow (in-memory checkpointer); only the Rust ledger and review lookups are faked.
const mocks = vi.hoisted(() => ({ ledger: vi.fn() }))
vi.mock("./tracing", () => ({
  noteVoiceFailure: () => {},
  withVoiceTracing: async (_: string, action: () => Promise<unknown>) =>
    action(),
}))
vi.mock("./checkpoint", async () => {
  const { MemorySaver } = await import("@langchain/langgraph")
  const saver = new MemorySaver()
  return {
    voiceCheckpointer: async () => saver,
    withVoiceThread: async (_: string, fn: () => unknown) => fn(),
  }
})
vi.mock("./bridge", async (original) => ({
  ...(await original<typeof import("./bridge")>()),
  assistantRequest: mocks.ledger,
}))
vi.mock("./review-target", () => ({ reviewTarget: async () => ({}) }))
import { proposeAction } from "./actions"
import { proposeBatch } from "./batches"

const workspaceId = "01950000-0000-7000-8000-000000000001"
const context = {
  token: "staff",
  workspaceId,
  permissions: ["workspace_admin"],
  apiUrl: "https://api.test/v1",
  sessionId: "",
  language: "en",
}
const action: PreparedAction = {
  operationId: "warehouses_create",
  summary: "Create warehouse",
  method: "POST",
  path: `/workspace/${workspaceId}/warehouses`,
  body: { name: "Back store" },
  write: true,
  idempotent: false,
}
const posts = (path: string) =>
  mocks.ledger.mock.calls.filter(
    ([, p, method]) => p === path && method === "POST"
  ).length

let n = 0
beforeEach(() => {
  vi.clearAllMocks()
  vi.stubEnv("ASSISTANT_BRIDGE_SECRET", "a".repeat(32))
  mocks.ledger.mockImplementation(async () => ({
    id: `01950000-0000-7000-8000-00000000${String(++n).padStart(4, "0")}`,
    session_id: "s",
    operation_id: "warehouses_create",
    state: "proposed",
    expires_at: new Date(Date.now() + 60000).toISOString(),
  }))
})

it("a second proposal while a review is open is refused before any ledger row is written", async () => {
  const ctx = { ...context, sessionId: "order-single" }
  expect((await proposeAction(action, ctx, new Map())).status).toBe(
    "requires_confirmation"
  )
  await expect(proposeAction(action, ctx, new Map())).rejects.toMatchObject({
    message: "Finish or cancel the pending review first.",
    status: 409,
  })
  expect(posts("/actions")).toBe(1)
})

it("a batch proposed while a review is open is refused before the batch is written", async () => {
  const ctx = { ...context, sessionId: "order-batch" }
  await proposeAction(action, ctx, new Map())
  const calls = [1, 2].map((i) => ({
    operationId: "warehouses_create",
    arguments: { body: { name: `Place ${i}` } },
  }))
  await expect(proposeBatch(calls, ctx, new Map())).rejects.toMatchObject({
    status: 409,
  })
  expect(posts("/batches")).toBe(0)
})
