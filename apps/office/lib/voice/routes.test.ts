import { beforeEach, expect, it, vi } from "vitest"
import { createHash } from "node:crypto"
import * as catalog from "./catalog"

const mocks = vi.hoisted(() => ({
  authorize: vi.fn(),
  session: vi.fn(),
  ledger: vi.fn(),
  verify: vi.fn(),
  execute: vi.fn(),
  transport: vi.fn(),
  propose: vi.fn(),
  confirmBatch: vi.fn(),
  proposeBatch: vi.fn(),
  importPrices: vi.fn(),
  failures: [] as unknown[][],
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
  noteVoiceFailure: (...args: unknown[]) => mocks.failures.push(args),
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
  readAssistantSession: mocks.session,
  requireAssistantSession: mocks.session,
  assistantRequest: mocks.ledger,
}))
vi.mock("./workflow", async (original) => ({
  ...(await original<typeof import("./workflow")>()),
  verifyReview: mocks.verify,
  executeAction: mocks.execute,
  cancelReview: vi.fn(),
  checkpointReview: vi.fn(),
}))
vi.mock("./actions", async (original) => ({
  ...(await original<typeof import("./actions")>()),
  proposeAction: mocks.propose,
  actionTransport: mocks.transport,
}))
vi.mock("./batches", async (original) => ({
  ...(await original<typeof import("./batches")>()),
  confirmBatch: mocks.confirmBatch,
  proposeBatch: mocks.proposeBatch,
}))
vi.mock("./file-import", async (original) => ({
  ...(await original<typeof import("./file-import")>()),
  importPrices: mocks.importPrices,
}))

import { POST as toolRequest } from "@/app/api/voice/tools/route"
import { POST as confirmationRequest } from "@/app/api/voice/confirm/route"

const context = {
  token: "staff",
  workspaceId: "01950000-0000-7000-8000-000000000009",
  permissions: ["workspace_admin"],
  apiUrl: "https://api.test/v1",
  sessionId: "s1",
}
const orderBody = {
  reference: "reviewed",
  lines: [{ product_id: context.workspaceId, quantity: "1" }],
}
const action = {
  operationId: "orders_create",
  summary: "Create order",
  method: "POST",
  path: "/workspace/w1/orders",
  write: true,
  idempotent: false,
  body: orderBody,
}
const request = (value: unknown) =>
  new Request("http://office.test/api/voice/tools", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(value),
  })

beforeEach(() => {
  vi.clearAllMocks()
  mocks.spans.length = 0
  mocks.modes.length = 0
  vi.stubEnv("ASSISTANT_ENABLED", "true")
  vi.stubEnv("ASSISTANT_WORKSPACE_ALLOWLIST", context.workspaceId)
  vi.stubEnv("ASSISTANT_BRIDGE_SECRET", "a".repeat(32))
  mocks.authorize.mockResolvedValue({ ...context })
  mocks.session.mockResolvedValue({
    id: "s1",
    language: "en",
    state: "active",
    conversation_id: "conv1",
  })
  mocks.verify.mockReturnValue({
    id: "a1",
    session: "s1",
    action,
    confirmationCode: "123456",
  })
  mocks.ledger.mockResolvedValue({
    id: "a1",
    session_id: "s1",
    operation_id: "orders_create",
    state: "proposed",
  })
  mocks.transport.mockResolvedValue(undefined)
  mocks.execute.mockResolvedValue({ status: "completed" })
  mocks.propose.mockResolvedValue({ status: "requires_confirmation" })
})

it("disabled assistant refuses new model work before authenticating or dispatching", async () => {
  vi.stubEnv("ASSISTANT_ENABLED", "false")
  const response = await toolRequest(
    request({
      command: "execute",
      workspaceId: "w1",
      sessionId: "s1",
      operationId: "orders_create",
      arguments: {},
    })
  )
  expect(response.status).toBe(503)
  expect(mocks.authorize).not.toHaveBeenCalled()
  expect(mocks.execute).not.toHaveBeenCalled()
})

it("preserves unavailable preparation status without creating or dispatching a draft", async () => {
  const prepare = vi
    .spyOn(catalog, "prepareAction")
    .mockImplementationOnce(() => {
      throw new catalog.ToolUnavailableError()
    })
  try {
    const response = await toolRequest(
      request({
        command: "execute",
        operationId: "orders_create",
        arguments: {},
        workspaceId: context.workspaceId,
        sessionId: "s1",
      })
    )
    expect(response.status).toBe(501)
    expect(mocks.propose).not.toHaveBeenCalled()
    expect(mocks.execute).not.toHaveBeenCalled()
    expect(mocks.ledger).not.toHaveBeenCalled()
  } finally {
    prepare.mockRestore()
  }
})

it("removing rollout admission blocks writes but preserves owned status and cancellation", async () => {
  vi.stubEnv(
    "ASSISTANT_WORKSPACE_ALLOWLIST",
    "01950000-0000-7000-8000-000000000008"
  )
  const blocked = await toolRequest(
    request({
      command: "execute",
      workspaceId: context.workspaceId,
      sessionId: "s1",
      operationId: "orders_create",
      arguments: {},
    })
  )
  expect(blocked.status).toBe(403)
  expect(mocks.execute).not.toHaveBeenCalled()
  const id = "01950000-0000-7000-8000-000000000001"
  mocks.ledger.mockResolvedValue({
    id,
    session_id: "s1",
    operation_id: "orders_create",
    state: "completed",
  })
  expect(
    (
      await toolRequest(
        request({
          command: "status",
          workspaceId: context.workspaceId,
          sessionId: "s1",
          actionId: id,
        })
      )
    ).status
  ).toBe(200)
  mocks.ledger.mockResolvedValue({
    id: "a1",
    session_id: "s1",
    operation_id: "orders_create",
    state: "proposed",
  })
  expect(
    (
      await confirmationRequest(
        request({
          workspaceId: context.workspaceId,
          sessionId: "s1",
          ticket: "ticket",
          decision: "cancel",
        })
      )
    ).status
  ).toBe(200)
  expect(mocks.ledger).toHaveBeenCalledWith(
    expect.anything(),
    "/actions/a1/cancel",
    "POST",
    { session_id: "s1" }
  )
})

it("batch refresh rechecks rollout admission before another child can start", async () => {
  mocks.verify.mockReturnValue({
    id: "batch",
    action: { ...action, operationId: "assistant_batch_prepare" },
  })
  mocks.confirmBatch.mockImplementation(async (...args) => {
    vi.stubEnv(
      "ASSISTANT_WORKSPACE_ALLOWLIST",
      "01950000-0000-7000-8000-000000000008"
    )
    await args[5]()
    throw new Error("Rollout change must stop this path")
  })
  const response = await confirmationRequest(
    request({
      workspaceId: context.workspaceId,
      sessionId: "s1",
      ticket: "ticket",
      decision: "approve",
    })
  )
  expect(response.status).toBe(403)
  expect(mocks.confirmBatch).toHaveBeenCalledOnce()
  expect(mocks.execute).not.toHaveBeenCalled()
})

it("disabled admission preserves the native read-only task-status alias", async () => {
  vi.stubEnv("ASSISTANT_ENABLED", "false")
  const id = "01950000-0000-7000-8000-000000000001"
  mocks.ledger.mockResolvedValue({
    id,
    session_id: "s1",
    operation_id: "orders_create",
    state: "completed",
  })
  const response = await toolRequest(
    request({
      command: "execute",
      operationId: "assistant_task_status",
      arguments: { actionId: id },
      workspaceId: context.workspaceId,
      sessionId: "s1",
    })
  )
  expect(response.status).toBe(200)
  expect(mocks.execute).not.toHaveBeenCalled()
})

it("model tools cannot commit or approve even with a copied review ticket", async () => {
  const response = await toolRequest(
    request({
      command: "commit",
      ticket: "copied",
      decision: "approve",
      confirmed: true,
      workspaceId: "w1",
      sessionId: "s1",
    })
  )
  expect(response.status).toBe(400)
  expect(mocks.execute).not.toHaveBeenCalled()
  expect(mocks.ledger).not.toHaveBeenCalled()
})

it("speech requires the exact localized action challenge and current action ownership", async () => {
  const payload = {
    ticket: "ticket",
    decision: "approve",
    method: "speech",
    eventId: "12",
    confirmationCode: "123456",
    locale: "en",
    utterance: "yes",
    workspaceId: "w1",
    sessionId: "s1",
  }
  expect((await confirmationRequest(request(payload))).status).toBe(400)
  expect(mocks.execute).not.toHaveBeenCalled()
  expect(
    mocks.ledger.mock.calls.some((call) => String(call[1]).endsWith("/approve"))
  ).toBe(false)
  mocks.ledger.mockResolvedValue({
    id: "a1",
    session_id: "another-session",
    operation_id: "orders_create",
    state: "proposed",
  })
  expect(
    (
      await confirmationRequest(
        request({ ...payload, utterance: "Confirm action 123456" })
      )
    ).status
  ).toBe(403)
  expect(mocks.execute).not.toHaveBeenCalled()
})

it("only a matched speech challenge sends the signed replay proof before execution", async () => {
  const response = await confirmationRequest(
    request({
      ticket: "ticket",
      decision: "approve",
      method: "speech",
      eventId: "12",
      confirmationCode: "123456",
      locale: "en",
      utterance: "Confirm action 123456",
      workspaceId: "w1",
      sessionId: "s1",
    })
  )
  expect(response.status).toBe(200)
  expect(mocks.ledger).toHaveBeenCalledWith(
    expect.anything(),
    "/actions/a1/approve",
    "POST",
    {
      session_id: "s1",
      approval_event_sha256: createHash("sha256").update("s1:12").digest("hex"),
    },
    true
  )
  expect(mocks.execute).toHaveBeenCalledTimes(1)
  expect(mocks.ledger.mock.invocationCallOrder.at(-1)).toBeLessThan(
    mocks.execute.mock.invocationCallOrder[0]!
  )
})

it("restores a fresh canonical review for the same owned unexpired JSON proposal without dispatch", async () => {
  mocks.ledger.mockResolvedValue({
    id: "a1",
    session_id: "s1",
    operation_id: "orders_create",
    state: "proposed",
    expires_at: new Date(Date.now() + 60000).toISOString(),
    proposal: {
      method: "POST",
      path: `/workspace/${context.workspaceId}/orders`,
      body: orderBody,
    },
  })
  const response = await toolRequest(
    request({
      command: "restore",
      actionId: "01950000-0000-7000-8000-000000000001",
      workspaceId: "w1",
      sessionId: "s1",
    })
  )
  expect(response.status).toBe(200)
  expect(await response.json()).toMatchObject({
    status: "requires_confirmation",
    data: { actionId: "a1" },
    review: {
      confirmationCode: "123456",
      details: { changes: { reference: "reviewed" } },
    },
  })
  expect(mocks.execute).not.toHaveBeenCalled()
  expect(mocks.ledger.mock.calls.some((call) => call[2] === "POST")).toBe(false)
})

it("authorizes new tool work against a live session inside one request span", async () => {
  await toolRequest(
    request({
      command: "execute",
      operationId: "orders_create",
      arguments: { body: orderBody },
      workspaceId: context.workspaceId,
      sessionId: "s1",
    })
  )
  expect(mocks.spans[0]).toBe("genesy_assistant_tools")
  expect(mocks.modes).toEqual(["live"])
})

it("traces confirmation approval inside one request span against a live session", async () => {
  await confirmationRequest(
    request({
      workspaceId: context.workspaceId,
      sessionId: "s1",
      ticket: "ticket",
      decision: "approve",
      method: "click",
    })
  )
  expect(mocks.spans).toEqual(["genesy_assistant_confirm", "approve"])
  expect(mocks.modes).toEqual(["live"])
})

it("labels a lookup with its human step for the status line", async () => {
  const response = await toolRequest(
    request({
      command: "execute",
      operationId: "orders_list",
      arguments: {},
      workspaceId: context.workspaceId,
      sessionId: "s1",
    })
  )
  expect(await response.json()).toMatchObject({
    status: "completed",
    steps: ["Checking: List orders"],
  })
})

it("narrates a proposed change in the session language without exposing the challenge", async () => {
  mocks.session.mockResolvedValue({
    id: "s1",
    language: "nl",
    state: "active",
    conversation_id: "conv1",
  })
  const response = await toolRequest(
    request({
      command: "execute",
      operationId: "orders_create",
      arguments: { body: orderBody },
      workspaceId: context.workspaceId,
      sessionId: "s1",
    })
  )
  const summary = catalog.getOperation("orders_create")!.summary
  expect(await response.json()).toMatchObject({
    status: "requires_confirmation",
    steps: [`Controleren: ${summary}`, "Je controle voorbereiden"],
    narration: "Dit ga ik doen. Lees de kaart en keur hem goed.",
  })
})

it("labels approved execution as running then finalizing", async () => {
  const response = await confirmationRequest(
    request({
      workspaceId: context.workspaceId,
      sessionId: "s1",
      ticket: "ticket",
      decision: "approve",
      method: "click",
    })
  )
  expect(await response.json()).toMatchObject({
    status: "completed",
    steps: ["Running it now", "Finalizing"],
  })
})

const execute = (operationId: string, args: unknown) =>
  toolRequest(
    request({
      command: "execute",
      operationId,
      arguments: args,
      workspaceId: context.workspaceId,
      sessionId: "s1",
    })
  )

it("returns the exact input schema when an undiscovered lookup has invalid arguments", async () => {
  const response = await execute("orders_get", { path: {} })
  expect(response.status).toBe(400)
  const body = await response.json()
  expect(body.message).toBeTruthy()
  expect(body.operation).toMatchObject({ id: "orders_get" })
  expect(
    body.operation.parameters.map((p: { name: string }) => p.name)
  ).toContain("order")
  expect(mocks.execute).not.toHaveBeenCalled()
})

it("never returns an inline schema for an effect with invalid arguments", async () => {
  const response = await execute("orders_create", { body: {} })
  expect(response.status).toBe(400)
  expect(await response.json()).not.toHaveProperty("operation")
  expect(mocks.propose).not.toHaveBeenCalled()
})

it("suggests permitted candidates for a guessed operation id", async () => {
  const response = await execute("order_list", {})
  expect(response.status).toBe(400)
  const body = await response.json()
  expect(body.candidates.map((tool: { id: string }) => tool.id)).toContain(
    "orders_list"
  )
  mocks.authorize.mockResolvedValue({ ...context, permissions: [] })
  const denied = await (await execute("order_list", {})).json()
  expect(
    denied.candidates.map((tool: { id: string }) => tool.id)
  ).not.toContain("orders_list")
})

it("discovery returns the top match's full input schema so execute can follow one search", async () => {
  const response = await toolRequest(
    request({
      command: "discover",
      query: "orders list",
      workspaceId: context.workspaceId,
      sessionId: "s1",
    })
  )
  const { tools } = await response.json()
  expect(tools[0]).toMatchObject({ id: "orders_list" })
  expect(tools[0].parameters.map((p: { name: string }) => p.name)).toContain(
    "page_size"
  )
  expect(tools.length).toBeGreaterThan(1)
  for (const rest of tools.slice(1)) expect(rest.parameters).toEqual([])
})

it("logs a request no tool can do, with the agent's short phrase, and changes nothing", async () => {
  mocks.failures.length = 0
  const response = await toolRequest(
    request({
      command: "discover",
      query: "unsupported:  book a delivery truck for Friday",
      workspaceId: context.workspaceId,
      sessionId: "s1",
    })
  )
  expect(await response.json()).toMatchObject({
    tools: [],
    reason: "unsupported",
  })
  expect(mocks.failures).toEqual([
    ["unsupported", undefined, "book a delivery truck for Friday"],
  ])
  expect(mocks.execute).not.toHaveBeenCalled()
})

const discover = async (query: string) =>
  (
    await (
      await toolRequest(
        request({
          command: "discover",
          query,
          workspaceId: context.workspaceId,
          sessionId: "s1",
        })
      )
    ).json()
  ).tools.map((tool: { id: string }) => tool.id)

it("discovers the price-list import only for staff who may change prices", async () => {
  expect(await discover("update prices from excel price list")).toContain(
    "assistant_import_prices"
  )
  expect(await discover("assistant_import_prices")).toEqual([
    "assistant_import_prices",
  ])
  mocks.authorize.mockResolvedValue({ ...context, permissions: [] })
  expect(await discover("import spreadsheet")).not.toContain(
    "assistant_import_prices"
  )
})

it("runs the price-list import with the shared files and refuses without permission", async () => {
  mocks.importPrices.mockResolvedValue({ status: "requires_confirmation" })
  const args = { attachments: { sheet: ["h"] }, fromRow: 51 }
  const response = await execute("assistant_import_prices", args)
  expect(await response.json()).toMatchObject({
    status: "requires_confirmation",
  })
  expect(mocks.importPrices).toHaveBeenCalledWith(
    args,
    expect.objectContaining({ sessionId: "s1" }),
    expect.any(Map)
  )
  mocks.authorize.mockResolvedValue({ ...context, permissions: [] })
  expect((await execute("assistant_import_prices", args)).status).toBe(403)
  expect(mocks.importPrices).toHaveBeenCalledOnce()
})

it("shows model-reported unclear lines on the card and never sends them as arguments", async () => {
  mocks.propose.mockResolvedValue({
    status: "requires_confirmation",
    review: { details: {} },
  })
  const unclear = [{ line: 4, seen: "Sugar ?? kg" }]
  const single = await (
    await execute("orders_create", { body: orderBody, unclear })
  ).json()
  expect(mocks.propose.mock.calls[0]![0]).not.toHaveProperty("unclear")
  expect(single.review.details.notIncluded).toEqual([
    { line: 4, text: "Sugar ?? kg", reason: "unclear" },
  ])
  mocks.proposeBatch.mockResolvedValue({
    status: "requires_confirmation",
    review: { details: {} },
  })
  const calls = [
    { operationId: "orders_create", arguments: { body: orderBody } },
  ]
  const batch = await (
    await execute("assistant_batch_prepare", { calls, unclear })
  ).json()
  expect(mocks.proposeBatch.mock.calls[0]![0]).toEqual(calls)
  expect(batch.review.details.notIncluded).toHaveLength(1)
  const bad = await execute("assistant_batch_prepare", {
    calls,
    unclear: "x".repeat(5),
  })
  expect(bad.status).toBe(400)
  expect(mocks.proposeBatch).toHaveBeenCalledOnce()
})
