import { beforeEach, expect, it, vi } from "vitest"
const mocks = vi.hoisted(() => ({ ledger: vi.fn(), execute: vi.fn() }))
vi.mock("./bridge", async (original) => ({
  ...(await original<typeof import("./bridge")>()),
  assistantRequest: mocks.ledger,
}))
vi.mock("./workflow", async (original) => ({
  ...(await original<typeof import("./workflow")>()),
  executeAction: mocks.execute,
  checkpointReview: vi.fn(),
}))
import {
  confirmBatch,
  nextPart,
  proposeBatch,
  BATCH_PREPARE_OPERATION,
} from "./batches"
import { createReview, verifyReview } from "./workflow"
import { confirmationPhrase } from "./confirmation"
import type { PreparedAction } from "./catalog"
const context = {
  token: "private-bearer",
  workspaceId: "01950000-0000-7000-8000-000000000001",
  permissions: ["workspace_admin"],
  apiUrl: "https://api.test/v1",
  sessionId: "session",
  language: "en",
}
const secret = "a".repeat(32)
const actions: PreparedAction[] = [1, 2].map((i) => ({
  operationId: "warehouses_create",
  summary: "Create warehouse",
  method: "POST",
  path: `/workspace/${context.workspaceId}/warehouses`,
  body: { name: `Place ${i}` },
  write: true,
  idempotent: false,
}))
function review(
  id: string,
  expires = new Date(Date.now() + 120000).toISOString(),
  extra: Record<string, unknown> = {}
) {
  const plan = {
    version: 1,
    fingerprint: "f".repeat(64),
    actions,
    actionIds: ["one", "two"],
    transports: [null, null],
    ...extra,
  }
  const action: PreparedAction = {
    operationId: BATCH_PREPARE_OPERATION,
    summary: "Fixed batch",
    method: "POST",
    path: "/assistant/batches",
    body: plan,
    write: true,
    idempotent: false,
  }
  return verifyReview(
    createReview(action, context, secret, "session", id, expires),
    context,
    secret
  )
}
function row(id: string, state = "proposed") {
  return {
    id,
    session_id: "session",
    fingerprint: "f".repeat(64),
    state,
    next_index: 0,
    action_count: 2,
    expires_at: new Date(Date.now() + 120000).toISOString(),
    steps: ["one", "two"].map((id) => ({
      id,
      session_id: "session",
      operation_id: "warehouses_create",
      state: "proposed",
      expires_at: new Date(Date.now() + 120000).toISOString(),
    })),
  }
}
const refresh = async () => ({ ...context })
beforeEach(() => {
  vi.clearAllMocks()
  mocks.execute.mockResolvedValue({ status: "completed" })
})
it("old spoken phrase cannot approve a newly bound batch", async () => {
  const old = review("old-batch")
  const next = review("new-batch")
  expect(next.confirmationCode).not.toBe(old.confirmationCode)
  mocks.ledger.mockResolvedValue(row("new-batch"))
  await expect(
    confirmBatch(
      next,
      {
        decision: "approve",
        method: "speech",
        eventId: "2",
        locale: "en",
        confirmationCode: old.confirmationCode,
        utterance: confirmationPhrase(old.confirmationCode, "en", "batch"),
      },
      context,
      new Map(),
      new AbortController().signal,
      refresh
    )
  ).rejects.toThrow("exact fixed batch")
  expect(
    mocks.ledger.mock.calls.some((call) => String(call[1]).endsWith("/approve"))
  ).toBe(false)
  expect(mocks.execute).not.toHaveBeenCalled()
})
it("changed immutable membership rejects before approval", async () => {
  const bound = review("batch")
  const changed = row("batch")
  changed.steps.reverse()
  mocks.ledger.mockResolvedValue(changed)
  await expect(
    confirmBatch(
      bound,
      { decision: "approve", method: "click" },
      context,
      new Map(),
      new AbortController().signal,
      refresh
    )
  ).rejects.toThrow("immutable")
  expect(mocks.execute).not.toHaveBeenCalled()
})
it.each(["running", "completed", "paused", "cancelled", "expired"])(
  "echo/new user event cannot restart %s batch",
  async (state) => {
    const bound = review("batch")
    mocks.ledger.mockResolvedValue(row("batch", state))
    await confirmBatch(
      bound,
      {
        decision: "approve",
        method: "speech",
        eventId: "99",
        locale: "en",
        confirmationCode: bound.confirmationCode,
        utterance: confirmationPhrase(bound.confirmationCode, "en", "batch"),
      },
      context,
      new Map(),
      new AbortController().signal,
      refresh
    )
    expect(mocks.execute).not.toHaveBeenCalled()
    expect(
      mocks.ledger.mock.calls.some(
        (call) =>
          String(call[1]).endsWith("/approve") ||
          String(call[1]).endsWith("/start")
      )
    ).toBe(false)
  }
)
it("expired batch ticket never reaches an execution path", () => {
  expect(() =>
    review("expired", new Date(Date.now() - 1000).toISOString())
  ).toThrow("expired")
})

it("model batches take up to 25 actions and a whole remaining count", async () => {
  const calls = (n: number) =>
    Array.from({ length: n }, () => ({ operationId: "nope", arguments: {} }))
  await expect(proposeBatch(calls(26), context, new Map())).rejects.toThrow(
    "Choose two through 25 exact actions."
  )
  for (const remaining of [0, 1.5, 10001, "3"])
    await expect(
      proposeBatch(calls(2), context, new Map(), 25, remaining)
    ).rejects.toThrow("remaining must be a whole number from 1 to 10000.")
  expect(mocks.ledger).not.toHaveBeenCalled()
})

it("only a completed part offers the next one, in the user's language", () => {
  const done = nextPart({ status: "completed", data: { steps: 2 } }, 12, "nl")
  expect(done.data).toEqual({ steps: 2, next: { remaining: 12 } })
  expect(done.narration).toContain("nog 12 wijzigingen")
  for (const status of ["paused", "failed", "partial", "unknown"] as const)
    expect(nextPart({ status }, 12)).toEqual({ status })
  expect(nextPart({ status: "completed" }, undefined)).toEqual({
    status: "completed",
  })
})

it("the signed remaining count reaches the completed outcome; a bad one is refused", async () => {
  mocks.ledger.mockImplementation(async (_c: unknown, path: string) =>
    row(path.split("/")[2]!)
  )
  const result = await confirmBatch(
    review("b-next", undefined, { remaining: 12 }),
    { decision: "approve", method: "click" },
    context,
    new Map(),
    new AbortController().signal,
    refresh
  )
  expect(result.data).toEqual({ next: { remaining: 12 } })
  await expect(
    confirmBatch(
      review("b-bad", undefined, { remaining: 0 }),
      { decision: "approve", method: "click" },
      context,
      new Map(),
      new AbortController().signal,
      refresh
    )
  ).rejects.toThrow("Invalid fixed batch review.")
})
