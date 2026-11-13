import { afterEach, beforeEach, expect, it, vi } from "vitest"
import { createRequests, createTurn, endedBy, SETTLE_MS, STALL_MS } from "./turn"

let ends: { kind: string; steps: number }[]
let turn: ReturnType<typeof createTurn>
beforeEach(() => {
  vi.useFakeTimers()
  ends = []
  turn = createTurn((kind, steps) => ends.push({ kind, steps }))
})
afterEach(() => vi.useRealTimers())

it("a narration line before a tool call does not end the turn", async () => {
  turn.begin()
  turn.agentLine() // "I'm checking negative stock."
  turn.toolStart()
  await vi.advanceTimersByTimeAsync(SETTLE_MS * 2)
  expect(ends).toEqual([])
  expect(turn.open).toBe(true)
  turn.toolEnd()
  turn.agentLine() // the real answer
  await vi.advanceTimersByTimeAsync(SETTLE_MS - 1)
  expect(ends).toEqual([])
  await vi.advanceTimersByTimeAsync(1)
  expect(ends).toEqual([{ kind: "answered", steps: 1 }])
  expect(turn.open).toBe(false)
})

it("ends with 'stalled' when nothing happens for the stall time after the last activity", async () => {
  turn.begin()
  turn.toolStart()
  turn.toolEnd()
  await vi.advanceTimersByTimeAsync(STALL_MS - 1000)
  turn.agentLine() // narration resets the clock …
  turn.toolStart()
  turn.toolEnd() // … and so does a tool event
  await vi.advanceTimersByTimeAsync(STALL_MS - 1)
  expect(ends).toEqual([])
  await vi.advanceTimersByTimeAsync(1)
  expect(ends).toEqual([{ kind: "stalled", steps: 2 }])
})

it("never stalls while a tool is still running (a status wait can take long)", async () => {
  turn.begin()
  turn.toolStart()
  await vi.advanceTimersByTimeAsync(STALL_MS * 3)
  expect(ends).toEqual([])
  expect(turn.open).toBe(true)
})

it("agent lines outside a turn and stop() end nothing", async () => {
  turn.agentLine()
  turn.begin()
  turn.stop()
  await vi.advanceTimersByTimeAsync(STALL_MS * 2)
  expect(ends).toEqual([])
  expect(turn.open).toBe(false)
})

// P7-3: the agent ran a tool on its own (no user turn) and then said nothing.
it("ends 'quiet' when the agent goes silent after a tool call of its own", async () => {
  turn.toolStart()
  turn.toolEnd()
  await vi.advanceTimersByTimeAsync(STALL_MS - 1)
  expect(ends).toEqual([])
  await vi.advanceTimersByTimeAsync(1)
  expect(ends.map((e) => e.kind)).toEqual(["quiet"])
  await vi.advanceTimersByTimeAsync(STALL_MS * 2)
  expect(ends).toHaveLength(1)
})

it("an agent line after its own tool call ends nothing", async () => {
  turn.toolStart()
  turn.toolEnd()
  turn.agentLine()
  await vi.advanceTimersByTimeAsync(STALL_MS * 2)
  expect(ends).toEqual([])
})

it("a user request during that wait takes over and ends as a user turn", async () => {
  turn.toolStart()
  turn.toolEnd()
  await vi.advanceTimersByTimeAsync(STALL_MS - 1000)
  turn.begin()
  await vi.advanceTimersByTimeAsync(STALL_MS)
  expect(ends.map((e) => e.kind)).toEqual(["stalled"])
})

it.each([
  [{ reason: "user" }, "user"],
  [{ reason: "agent" }, "ended_by_agent"],
  [
    {
      reason: "error",
      message: "Max duration",
      context: { type: "max_duration_exceeded" },
    },
    "time_limit",
  ],
  [{ reason: "error", closeReason: "Maximum duration exceeded" }, "time_limit"],
  [
    { reason: "error", message: "socket closed", closeCode: 1006 },
    "connection_lost",
  ],
  [undefined, "connection_lost"],
])("names why a conversation ended: %j", (details, kind) => {
  expect(endedBy(details)).toBe(kind)
})

it("counts as working only once the agent has started (narration or a tool), so rapid follow-ups still merge", () => {
  turn.begin()
  expect(turn.working).toBe(false)
  turn.agentLine()
  expect(turn.working).toBe(true)
  turn.begin()
  expect(turn.working).toBe(false)
  turn.toolStart()
  expect(turn.working).toBe(true)
  turn.stop()
  expect(turn.working).toBe(false)
})

it("queued tools keep an answer open until every dataset has settled", async () => {
  turn.begin()
  const first = turn.toolStart()
  const second = turn.toolStart()
  const third = turn.toolStart()
  turn.toolEnd(first)
  turn.agentLine()
  await vi.advanceTimersByTimeAsync(SETTLE_MS * 2)
  expect(ends).toEqual([])
  turn.toolEnd(second)
  turn.toolEnd(third)
  await vi.advanceTimersByTimeAsync(SETTLE_MS)
  expect(ends).toEqual([{ kind: "answered", steps: 3 }])
})

it("old-request tools cannot end or hold a new request", async () => {
  turn.begin()
  const old = turn.toolStart()
  turn.begin()
  turn.toolEnd(old)
  turn.agentLine()
  await vi.advanceTimersByTimeAsync(SETTLE_MS)
  expect(ends).toEqual([{ kind: "answered", steps: 0 }])
})

it("typed echoes and replayed provider events preserve canonical request identity", () => {
  let ordinal = 0
  const requests = createRequests(() => `request-${++ordinal}`)
  expect(requests.typed("Sales today")).toBe("request-1")
  expect(requests.heard("Sales today", "1")).toEqual({ id: "request-1", fresh: false })
  expect(requests.heard("Sales today", "1")).toEqual({ id: "request-1", fresh: false })
  expect(requests.heard("Sales today", "2")).toEqual({ id: "request-2", fresh: true })
})

it("late typed echoes cannot replace the identity of a newer typed request", () => {
  let ordinal = 0
  const requests = createRequests(() => `request-${++ordinal}`)
  requests.typed("Sales")
  requests.typed("Stock")
  expect(requests.heard("Sales", "1")).toEqual({ id: "request-1", fresh: false })
  expect(requests.current).toBe("request-2")
  expect(requests.heard("Stock", "2")).toEqual({ id: "request-2", fresh: false })
  requests.clear()
  expect(requests.current).toBeNull()
  expect(requests.heard("Sales", "1")).toEqual({ id: "request-3", fresh: true })
})

it("a completed typed request cannot swallow a later identical spoken request", () => {
  let ordinal = 0
  const requests = createRequests(() => `request-${++ordinal}`)
  requests.typed("Sales")
  requests.finish()
  expect(requests.heard("Sales", "1")).toEqual({ id: "request-2", fresh: true })
})
