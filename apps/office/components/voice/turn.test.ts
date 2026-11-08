import { afterEach, beforeEach, expect, it, vi } from "vitest"
import { createTurn, endedBy, SETTLE_MS, STALL_MS } from "./turn"

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
