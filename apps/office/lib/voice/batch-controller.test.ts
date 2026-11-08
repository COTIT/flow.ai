import { beforeEach, expect, it, vi } from "vitest"
import type { PreparedAction } from "./catalog"
import {
  executeBatchController,
  batchStatus,
  batchFailure,
  type StoredBatch,
} from "./batch-controller"
import { ToolUnavailableError } from "./catalog"

it.each(["en", "nl", "sw"])(
  "preserves unavailable batch preparation status in %s",
  (language) => {
    expect(batchFailure(new ToolUnavailableError(), language).status).toBe(501)
  }
)
const ledger = vi.hoisted(() => vi.fn())
vi.mock("./bridge", async (original) => ({
  ...(await original<typeof import("./bridge")>()),
  assistantRequest: ledger,
}))
const context = {
  token: "staff-bearer-private",
  workspaceId: "workspace",
  permissions: ["workspace_admin"],
  apiUrl: "https://api.test/v1",
  sessionId: "session",
}
const actions: PreparedAction[] = [1, 2].map((i) => ({
  operationId: "warehouses_create",
  summary: "Create warehouse",
  write: true,
  idempotent: false,
  method: "POST",
  path: "/workspace/workspace/warehouses",
  body: { name: `Warehouse ${i}` },
}))
let batch: StoredBatch
let timeline: string[]
beforeEach(() => {
  vi.clearAllMocks()
  timeline = []
  batch = {
    id: "batch",
    session_id: "session",
    fingerprint: "hash",
    state: "approved",
    next_index: 0,
    action_count: 2,
    expires_at: new Date(Date.now() + 120000).toISOString(),
    steps: [1, 2].map((i) => ({
      id: `action-${i}`,
      session_id: "session",
      operation_id: "warehouses_create",
      state: "proposed",
      expires_at: new Date(Date.now() + 120000).toISOString(),
    })),
  }
  ledger.mockImplementation(
    async (
      _: unknown,
      path: string,
      _method?: string,
      value?: { action_id?: string; reason?: string }
    ) => {
      if (path.endsWith("/start")) {
        batch.state = "running"
        return { batch, execution_token: "c".repeat(64) }
      }
      if (path.endsWith("/advance")) {
        timeline.push(`advance:${value?.action_id}`)
        batch.steps[batch.next_index]!.state = "completed"
        batch.next_index++
        if (batch.next_index === 2) batch.state = "completed"
        return batch
      }
      if (path.endsWith("/pause")) {
        timeline.push(`pause:${value?.reason}`)
        batch.state = "paused"
        batch.reason = value?.reason
        return batch
      }
      return batch
    }
  )
})
function run(signal = new AbortController().signal) {
  return {
    batch,
    actions,
    context,
    files: new Map<string, Blob>(),
    signal,
    refresh: vi.fn(async () => ({ ...context })),
  }
}
it("dispatches exact ordered children only after verified advance", async () => {
  const backend = vi.fn(
    async (_: PreparedAction, _context: unknown, id: string) => {
      timeline.push(`dispatch:${id}`)
      return { status: "completed" as const }
    }
  )
  const result = await executeBatchController(run(), backend)
  expect(result.status).toBe("completed")
  expect(timeline).toEqual([
    "dispatch:action-1",
    "advance:action-1",
    "dispatch:action-2",
    "advance:action-2",
  ])
  expect(JSON.stringify(result)).not.toContain("c".repeat(64))
  expect(JSON.stringify(result)).not.toContain(context.token)
})
it.each(["unknown", "failed", "queued", "partial"] as const)(
  "pauses remainder after %s without dispatching another child",
  async (status) => {
    const backend = vi.fn(async () => ({ status }))
    const result = await executeBatchController(run(), backend)
    expect(backend).toHaveBeenCalledTimes(1)
    expect(batch.state).toBe("paused")
    expect(result.status).not.toBe("completed")
    expect(timeline.some((value) => value.startsWith("advance:"))).toBe(false)
  }
)
it("disconnect during admitted action stops every future dispatch", async () => {
  const abort = new AbortController()
  let release!: (value: { status: "completed" }) => void
  const pending = new Promise<{ status: "completed" }>((resolve) => {
    release = resolve
  })
  const backend = vi.fn(() => pending)
  const result = executeBatchController(run(abort.signal), backend)
  await vi.waitFor(() => expect(backend).toHaveBeenCalledTimes(1))
  abort.abort()
  release({ status: "completed" })
  await result
  expect(backend).toHaveBeenCalledTimes(1)
  expect(batch.state).toBe("paused")
  expect(timeline).toContain("pause:disconnected")
})
it("lost live authorization between children stops remaining work", async () => {
  const scope = run()
  scope.refresh
    .mockResolvedValueOnce({ ...context })
    .mockRejectedValueOnce(new Error("revoked"))
  const backend = vi.fn(async () => ({ status: "completed" as const }))
  await executeBatchController(scope, backend)
  expect(backend).toHaveBeenCalledTimes(1)
  expect(batch.state).toBe("paused")
})

it("raw 2xx schema failure pauses before the next admission and remains unknown", async () => {
  const original = ledger.getMockImplementation()!
  ledger.mockImplementation(async (...args: unknown[]) => {
    if (String(args[1]).endsWith("/pause")) batch.steps[0]!.state = "unknown"
    return original(...args)
  })
  const request = vi.spyOn(globalThis, "fetch").mockImplementation(async () => {
    // Native guard saw 201; the response contract still lacks its required created id.
    batch.steps[0]!.state = "completed"
    return Response.json({ message: "ok" }, { status: 201 })
  })
  try {
    const result = await executeBatchController(run())
    expect(request).toHaveBeenCalledTimes(1)
    expect(timeline).toContain("pause:unknown")
    expect(timeline.some((item) => item.startsWith("advance:"))).toBe(false)
    expect(result.status).toBe("unknown")
    expect(JSON.stringify(result)).not.toContain("private-bearer")
  } finally {
    request.mockRestore()
  }
})

it.each([
  ["nl", "Vaste reeks: 2 acties", "Bekijk de exacte vaste reeks in Office."],
  ["sw", "Kundi maalumu: hatua 2", "Kagua kundi maalumu halisi katika Office."],
])(
  "localizes public batch summary and review guidance for %s",
  (language, summary, message) => {
    const result = batchStatus(batch, language)
    expect(result.data).toMatchObject({ summary })
    expect(result.message).toBe(message)
  }
)
