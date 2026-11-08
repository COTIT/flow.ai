import { beforeEach, expect, it, vi } from "vitest"
import type { PreparedAction } from "./catalog"

const mocks = vi.hoisted(() => ({
  spans: [] as string[],
  ledger: vi.fn(),
  target: vi.fn(),
  checkpoint: vi.fn(),
}))
vi.mock("./tracing", () => ({
  withVoiceTracing: async (name: string, action: () => Promise<unknown>) => {
    mocks.spans.push(name)
    return action()
  },
}))
vi.mock("./bridge", async (original) => ({
  ...(await original<typeof import("./bridge")>()),
  assistantRequest: mocks.ledger,
}))
vi.mock("./review-target", () => ({ reviewTarget: mocks.target }))
vi.mock("./workflow", async (original) => ({
  ...(await original<typeof import("./workflow")>()),
  checkpointReview: mocks.checkpoint,
}))
import { proposeAction } from "./actions"

const context = {
  token: "staff",
  workspaceId: "w1",
  permissions: ["workspace_admin"],
  apiUrl: "https://api.test/v1",
  sessionId: "s1",
  language: "en",
}
const action: PreparedAction = {
  operationId: "orders_create",
  summary: "Create order",
  method: "POST",
  path: "/workspace/w1/orders",
  body: { reference: "r" },
  write: true,
  idempotent: false,
}

beforeEach(() => {
  vi.clearAllMocks()
  mocks.spans.length = 0
  vi.stubEnv("ASSISTANT_BRIDGE_SECRET", "a".repeat(32))
  mocks.ledger.mockResolvedValue({
    id: "01950000-0000-7000-8000-000000000001",
    session_id: "s1",
    operation_id: "orders_create",
    state: "proposed",
    expires_at: new Date(Date.now() + 60000).toISOString(),
  })
  mocks.target.mockResolvedValue({})
  mocks.checkpoint.mockResolvedValue(undefined)
})

it("traces the draft, its review target lookup and ticket signing as separate spans", async () => {
  const result = await proposeAction(action, context, new Map())
  expect(result.status).toBe("requires_confirmation")
  expect(mocks.spans).toEqual(["propose", "review_target", "review_ticket"])
})
