import { afterEach, describe, expect, it, vi } from "vitest"
import {
  createOutcomeWait,
  readRecovery,
  saveRecovery,
  sessionLanguage,
  RECOVERY_KEY,
} from "./recovery"

function storage() {
  const values = new Map<string, string>()
  return {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => void values.set(key, value),
    removeItem: (key: string) => void values.delete(key),
  }
}
afterEach(() => vi.useRealTimers())

describe("private task recovery", () => {
  it("stores only opaque session/workspace IDs, never supplied extra values", () => {
    const tab = storage()
    saveRecovery(tab, {
      sessionId: "session-one",
      workspaceId: "w1",
      transcript: "private",
      ticket: "secret",
    } as never)
    expect(JSON.parse(tab.getItem(RECOVERY_KEY)!)).toEqual({
      sessionId: "session-one",
      workspaceId: "w1",
    })
    expect(readRecovery(tab, "w1")).toEqual({
      sessionId: "session-one",
      workspaceId: "w1",
    })
    expect(readRecovery(tab, "w2")).toBeNull()
  })
  it("rejects corrupt or content-bearing records without blocking the UI", () => {
    const tab = storage()
    for (const value of [
      "bad json",
      '{"sessionId":"s","workspaceId":"w1","messages":[]}',
      '{"sessionId":"https://elsewhere","workspaceId":"w1"}',
      "null",
    ]) {
      tab.setItem(RECOVERY_KEY, value)
      expect(readRecovery(tab, "w1")).toBeNull()
    }
    const blocked = {
      getItem: () => {
        throw new Error("blocked")
      },
      setItem: () => {
        throw new Error("blocked")
      },
      removeItem: () => {},
    }
    expect(readRecovery(blocked, "w1")).toBeNull()
    expect(() =>
      saveRecovery(blocked, { sessionId: "s", workspaceId: "w1" })
    ).not.toThrow()
  })
  it("fixes locale at conversation start using supported agent overrides", () => {
    expect(["en", "nl", "sw"].map(sessionLanguage)).toEqual(["en", "nl", "sw"])
    expect(sessionLanguage("unexpected")).toBe("en")
  })
  it("resolves native status tool once with actual outcome or bounded wait status", async () => {
    vi.useFakeTimers()
    const watch = createOutcomeWait({ status: "awaiting_review" })
    watch.finish({ status: "completed", message: "Saved." })
    watch.finish({ status: "failed" })
    await expect(watch.result).resolves.toEqual({
      status: "completed",
      message: "Saved.",
    })
    await vi.advanceTimersByTimeAsync(80000)
    const timeout = createOutcomeWait({ status: "awaiting_review" })
    await vi.advanceTimersByTimeAsync(79999)
    let settled = false
    void timeout.result.then(() => {
      settled = true
    })
    await Promise.resolve()
    expect(settled).toBe(false)
    await vi.advanceTimersByTimeAsync(1)
    await expect(timeout.result).resolves.toEqual({ status: "awaiting_review" })
  })
})
