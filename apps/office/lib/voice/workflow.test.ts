import { expect, it, vi } from "vitest"
import type { PreparedAction } from "./catalog"
import {
  checkpointReview,
  createReview,
  executeAction,
  verifyReview,
} from "./workflow"
import { voiceCheckpointer } from "./checkpoint"

const spans = vi.hoisted(() => [] as string[])
vi.mock("./tracing", () => ({
  withVoiceTracing: async (name: string, action: () => Promise<unknown>) => {
    spans.push(name)
    return action()
  },
}))
vi.mock("./checkpoint", async () => {
  const { MemorySaver } = await import("@langchain/langgraph")
  const saver = new MemorySaver()
  return {
    voiceCheckpointer: async () => saver,
    withVoiceThread: async (_: string, fn: () => unknown) => fn(),
  }
})

const context = {
  token: "private-bearer",
  workspaceId: "w1",
  permissions: ["workspace_admin"],
  apiUrl: "https://api.test/v1",
  sessionId: "session-one",
}
const action: PreparedAction = {
  operationId: "orders_create",
  summary: "Create order",
  method: "POST",
  path: "/workspace/w1/orders",
  body: { reference: "private-business-value" },
  write: true,
  idempotent: false,
}

it("binds review to exact action, login, workspace and application session", () => {
  const ticket = createReview(
    action,
    context,
    "secret",
    "session-one",
    "action-one"
  )
  expect(verifyReview(ticket, context, "secret").action).toEqual(action)
  expect(verifyReview(ticket, context, "secret")).toMatchObject({
    confirmationCode: expect.stringMatching(/^[0-9]{6}$/),
  })
  expect(() =>
    verifyReview(ticket, { ...context, token: "other" }, "secret")
  ).toThrow()
  expect(() =>
    verifyReview(ticket, { ...context, workspaceId: "other" }, "secret")
  ).toThrow()
  expect(() =>
    verifyReview(ticket, { ...context, sessionId: "other" }, "secret")
  ).toThrow()
  expect(() => verifyReview(ticket + "x", context, "secret")).toThrow()
})

it("persists review interrupt without dispatch and resumes an approved action once", async () => {
  const ctx = { ...context, sessionId: "workflow-success" }
  const backend = vi.fn(async () => ({
    status: "completed" as const,
    data: { private: "result-never-checkpointed" },
  }))
  await checkpointReview(action, ctx, "action-success")
  expect(backend).not.toHaveBeenCalled()
  expect(
    (await executeAction(action, ctx, "action-success", backend)).status
  ).toBe("completed")
  const saver = await voiceCheckpointer()
  const state = await saver.getTuple({
    configurable: { thread_id: ctx.sessionId },
  })
  const serialized = JSON.stringify(state)
  expect(serialized).not.toContain(context.token)
  expect(serialized).not.toContain("private-business-value")
  expect(serialized).not.toContain("result-never-checkpointed")
  expect(backend).toHaveBeenCalledTimes(1)
})

it("does not dispatch a write without a matching persisted review", async () => {
  const backend = vi.fn(async () => ({ status: "completed" as const }))
  await expect(
    executeAction(
      action,
      { ...context, sessionId: "no-review" },
      "unknown-action",
      backend
    )
  ).rejects.toThrow()
  expect(backend).not.toHaveBeenCalled()
})

it("does not retry an uncertain write through graph retry behavior", async () => {
  const ctx = { ...context, sessionId: "workflow-unknown" }
  const backend = vi.fn(async () => {
    throw new Error("private downstream failure")
  })
  await checkpointReview(action, ctx, "action-unknown")
  expect(
    (await executeAction(action, ctx, "action-unknown", backend)).status
  ).toBe("unknown")
  expect(backend).toHaveBeenCalledTimes(1)
})

it("batch checkpoint stores ordered references without private drafts or controller tokens", async () => {
  const ctx = { ...context, sessionId: "batch-checkpoint" }
  const batchAction = {
    ...action,
    operationId: "assistant_batch_prepare",
    body: {
      actions: [{ reference: "batch-private-business-value" }],
      token: "batch-private-controller-token",
    },
  }
  await checkpointReview(batchAction, ctx, "batch-reference", [
    "step-one",
    "step-two",
  ])
  await executeAction(batchAction, ctx, "batch-reference", async () => ({
    status: "partial",
  }))
  const saver = await voiceCheckpointer()
  const tuple = await saver.getTuple({
    configurable: { thread_id: ctx.sessionId },
  })
  const snapshot = JSON.stringify(tuple)
  expect(snapshot).toContain("step-one")
  expect(snapshot).toContain("step-two")
  expect(snapshot).not.toContain("batch-private-business-value")
  expect(snapshot).not.toContain("batch-private-controller-token")
  expect(snapshot).not.toContain(context.token)
})

it("traces checkpoint state reads inside the review span", async () => {
  spans.length = 0
  await checkpointReview(
    action,
    { ...context, sessionId: "session-spans" },
    "action-spans"
  )
  expect(spans).toEqual(["genesy_assistant_review", "checkpoint_state"])
})

it("a stale ticket for another action cannot consume the pending review", async () => {
  const ctx = { ...context, sessionId: "workflow-stale" }
  const backend = vi.fn(async () => ({ status: "completed" as const }))
  await checkpointReview(action, ctx, "action-pending")
  await expect(
    executeAction(action, ctx, "action-stale", backend)
  ).rejects.toMatchObject({ status: 409 })
  expect(backend).not.toHaveBeenCalled()
  expect(
    (await executeAction(action, ctx, "action-pending", backend)).status
  ).toBe("completed")
  await expect(
    executeAction(action, ctx, "action-pending", backend)
  ).rejects.toMatchObject({ status: 409 })
  expect(backend).toHaveBeenCalledTimes(1)
})

it("refuses execution with 409 when the session has no persisted review", async () => {
  const backend = vi.fn(async () => ({ status: "completed" as const }))
  await expect(
    executeAction(
      action,
      { ...context, sessionId: "workflow-empty" },
      "action-none",
      backend
    )
  ).rejects.toMatchObject({ status: 409 })
  expect(backend).not.toHaveBeenCalled()
})

it("returns only after every checkpoint write has finished", async () => {
  const saver = await voiceCheckpointer()
  let started = 0
  let finished = 0
  for (const method of ["put", "putWrites"] as const) {
    const original = saver[method].bind(saver) as (
      ...args: unknown[]
    ) => Promise<unknown>
    vi.spyOn(saver, method).mockImplementation((async (
      ...args: unknown[]
    ) => {
      started++
      await new Promise((resolve) => setTimeout(resolve, 20))
      const result = await original(...args)
      finished++
      return result
    }) as never)
  }
  try {
    const ctx = { ...context, sessionId: "workflow-durable" }
    await checkpointReview(action, ctx, "action-durable")
    expect(started).toBeGreaterThan(0)
    expect(finished).toBe(started)
    await executeAction(
      action,
      ctx,
      "action-durable",
      vi.fn(async () => ({ status: "completed" as const }))
    )
    expect(finished).toBe(started)
  } finally {
    vi.restoreAllMocks()
  }
})
