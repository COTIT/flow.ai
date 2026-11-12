import { describe, expect, it, vi } from "vitest"

import { createSyncGuard } from "./use-sync-guard"

describe("sync guard", () => {
  it("runs the first call and ignores a second one in the same tick", async () => {
    const guard = createSyncGuard()
    const start = vi.fn(() => Promise.resolve("done"))
    const first = guard(start)
    const second = guard(start)
    expect(second).toBeUndefined()
    await first
    expect(start).toHaveBeenCalledTimes(1)
  })

  it("lets the next call through once the first settled", async () => {
    const guard = createSyncGuard()
    const start = vi.fn(() => Promise.resolve())
    await guard(start)
    await guard(start)
    expect(start).toHaveBeenCalledTimes(2)
  })

  it("clears after a rejection too", async () => {
    const guard = createSyncGuard()
    await guard(() => Promise.reject(new Error("no")))?.catch(() => undefined)
    const start = vi.fn(() => Promise.resolve())
    await guard(start)
    expect(start).toHaveBeenCalledTimes(1)
  })

  it("clears when start throws before returning a promise", () => {
    const guard = createSyncGuard()
    expect(() =>
      guard(() => {
        throw new Error("boom")
      })
    ).toThrow("boom")
    const start = vi.fn(() => Promise.resolve())
    guard(start)
    expect(start).toHaveBeenCalledTimes(1)
  })
})
