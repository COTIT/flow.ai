import { beforeEach, expect, it, vi } from "vitest"
import type { PreparedAction } from "./catalog"

// Real review workflow (in-memory checkpointer); only the Rust ledger and review lookups are faked.
const mocks = vi.hoisted(() => ({ ledger: vi.fn(), free: vi.fn() }))
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
// The precheck can be skipped once to replay the race: another review lands after it passed.
vi.mock("./workflow", async (original) => {
  const real = await original<typeof import("./workflow")>()
  mocks.free.mockImplementation(real.assertReviewFree)
  return { ...real, assertReviewFree: (...args: Parameters<typeof real.assertReviewFree>) => mocks.free(...args) }
})
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
  mocks.ledger.mockReset()
  vi.stubEnv("ASSISTANT_BRIDGE_SECRET", "a".repeat(32))
  mocks.ledger.mockImplementation(async (_: unknown, path: string) => {
    const row = () => ({
      id: `01950000-0000-7000-8000-00000000${String(++n).padStart(4, "0")}`,
      session_id: "s",
      operation_id: "warehouses_create",
      state: "proposed",
      expires_at: new Date(Date.now() + 60000).toISOString(),
    })
    return path === "/batches"
      ? { ...row(), fingerprint: "f".repeat(64), next_index: 0, action_count: 2, steps: [row(), row()] }
      : row()
  })
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

it("a review that lands between the check and the ledger write cancels the new row (no orphan)", async () => {
  const ctx = { ...context, sessionId: "order-race-single" }
  await proposeAction(action, ctx, new Map())
  mocks.free.mockResolvedValueOnce(undefined)
  await expect(proposeAction(action, ctx, new Map())).rejects.toMatchObject({ status: 409 })
  const write = mocks.ledger.mock.calls.map((call) => call[1]).lastIndexOf("/actions")
  const second = (await mocks.ledger.mock.results[write]!.value) as { id: string }
  expect(mocks.ledger).toHaveBeenLastCalledWith(ctx, `/actions/${second.id}/cancel`, "POST", {
    session_id: "order-race-single",
  })
})

it("the same race for a batch cancels the new batch", async () => {
  const ctx = { ...context, sessionId: "order-race-batch" }
  await proposeAction(action, ctx, new Map())
  mocks.free.mockResolvedValueOnce(undefined)
  const calls = [1, 2].map((i) => ({
    operationId: "warehouses_create",
    arguments: { body: { name: `Place ${i}` } },
  }))
  await expect(proposeBatch(calls, ctx, new Map())).rejects.toMatchObject({ status: 409 })
  const batch = mocks.ledger.mock.calls.findIndex(([, p]) => p === "/batches")
  const stored = (await mocks.ledger.mock.results[batch]!.value) as { id: string }
  expect(mocks.ledger).toHaveBeenLastCalledWith(ctx, `/batches/${stored.id}/cancel`, "POST", {
    session_id: "order-race-batch",
    reason: "cancelled",
  })
})
