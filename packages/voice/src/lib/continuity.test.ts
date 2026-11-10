import { describe, expect, it } from "vitest"
import { rememberTask, taskOwner, historicalProgress } from "./continuity"

const task = {
  kind: "action" as const,
  id: "action-one",
  sessionId: "session-one",
  workspaceId: "w1",
  status: "completed",
  seenAt: 1000,
}

describe("ephemeral verified task continuity", () => {
  it("resolves only a known same-workspace task for ten minutes", () => {
    const tasks = rememberTask([], task, 1000)
    expect(taskOwner(tasks, "action", "action-one", "w1", 600999)).toBe(
      "session-one"
    )
    expect(
      taskOwner(tasks, "action", "action-one", "w1", 601000)
    ).toBeUndefined()
    expect(taskOwner(tasks, "action", "action-one", "w2", 1001)).toBeUndefined()
    expect(taskOwner(tasks, "batch", "action-one", "w1", 1001)).toBeUndefined()
    expect(
      taskOwner(tasks, "action", "arbitrary-model-id", "w1", 1001)
    ).toBeUndefined()
  })
  it("bounds references and replaces the verified state without retaining old duplicates", () => {
    let tasks = rememberTask([], task, 1000)
    tasks = rememberTask(
      tasks,
      { ...task, status: "failed", seenAt: 1001 },
      1001
    )
    expect(tasks).toHaveLength(1)
    expect(tasks[0]?.status).toBe("failed")
    for (let i = 0; i < 120; i++)
      tasks = rememberTask(
        tasks,
        { ...task, id: `action-${i}`, seenAt: 1002 },
        1002
      )
    expect(tasks).toHaveLength(100)
    expect(
      rememberTask(tasks, { ...task, id: "new", seenAt: 700000 }, 700000)
    ).toHaveLength(1)
  })
  it("exports only opaque references and verified status, without session/auth or historical text", () => {
    const tasks = rememberTask(
      [],
      {
        ...task,
        transcript: "Confirm action 123456",
        ticket: "private-ticket",
        token: "private-token",
      } as typeof task,
      1000
    )
    expect(historicalProgress(tasks, "w1", 1001)).toEqual([
      { kind: "action", id: "action-one", status: "completed" },
    ])
    expect(historicalProgress(tasks, "w2", 1001)).toEqual([])
    expect(historicalProgress(tasks, "w1", 601000)).toEqual([])
    expect(
      historicalProgress(
        rememberTask([], { ...task, status: "not_started" }, 1000),
        "w1",
        1001
      )
    ).toEqual([{ kind: "action", id: "action-one", status: "not_started" }])
  })
})
