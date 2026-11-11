import { describe, expect, it } from "vitest"

import { outcomeKind, settled, unseenOutcomes } from "./recovered-outcomes"

const task = (status: string, actionId = "a-1", summary = "Set stock levels") => ({
  status,
  data: { actionId, summary },
})

describe("unseenOutcomes", () => {
  it("lists what ran, failed or is unknown, not what still waits or never ran", () => {
    const tasks = [
      task("completed", "a"),
      task("failed", "b"),
      task("unknown", "c"),
      task("awaiting_review", "d"),
      task("cancelled", "e"),
      task("expired", "f"),
    ]
    expect(unseenOutcomes(tasks, []).map((t) => t.id)).toEqual(["a", "b", "c"])
  })

  it("skips what this owner has already seen", () => {
    expect(unseenOutcomes([task("completed", "a"), task("failed", "b")], ["a"]).map((t) => t.id)).toEqual(["b"])
  })

  it("uses the batch id for a batch and skips tasks without an id", () => {
    const batch = { status: "completed", data: { batchId: "b-1", kind: "batch" as const } }
    expect(unseenOutcomes([batch, { status: "completed" }], []).map((t) => t.id)).toEqual(["b-1"])
  })

  it("one line per batch, however many of its steps are listed", () => {
    const step = { status: "completed", data: { batchId: "b-1", actionId: "s" } }
    expect(unseenOutcomes([step, { ...step, data: { ...step.data, actionId: "t" } }], [])).toHaveLength(1)
  })

  it("shows at most three", () => {
    const many = ["a", "b", "c", "d"].map((id) => task("completed", id))
    expect(unseenOutcomes(many, [])).toHaveLength(3)
  })
})

describe("outcomeKind", () => {
  it("names what happened; anything not plainly done or failed must be checked", () => {
    expect(outcomeKind("completed")).toBe("ran")
    expect(outcomeKind("failed")).toBe("failed")
    expect(outcomeKind("unknown")).toBe("unconfirmed")
    expect(outcomeKind("partial")).toBe("unconfirmed")
  })
})

describe("settled", () => {
  const run = (status: string, state: string) => ({ status, data: { kind: "batch" as const, batchId: "b-1", state } })
  it("a run still going or paused is not settled, so it is shown again with its final outcome", () => {
    expect(settled(run("running", "running"))).toBe(false)
    expect(settled(run("partial", "paused"))).toBe(false)
    expect(settled(task("running"))).toBe(false)
    expect(settled(task("queued"))).toBe(false)
  })
  it("a final outcome is settled: shown once, then seen", () => {
    expect(settled(run("completed", "completed"))).toBe(true)
    expect(settled(run("partial", "cancelled"))).toBe(true)
    for (const status of ["completed", "failed", "unknown", "cancelled", "expired"])
      expect(settled(task(status))).toBe(true)
  })
})
