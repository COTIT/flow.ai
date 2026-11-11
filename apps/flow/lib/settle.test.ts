import { afterEach, describe, expect, it, vi } from "vitest"

import { settleWithin } from "./settle"

afterEach(() => vi.useRealTimers())

describe("settleWithin", () => {
  it("resolves as soon as the work resolves", async () => {
    await expect(
      settleWithin(Promise.resolve("x"), 1000)
    ).resolves.toBeUndefined()
  })

  it("resolves (does not throw) when the work fails", async () => {
    await expect(
      settleWithin(Promise.reject(new Error("down")), 1000)
    ).resolves.toBeUndefined()
  })

  it("stops waiting for work that never settles", async () => {
    vi.useFakeTimers()
    let done = false
    void settleWithin(new Promise(() => undefined), 3000).then(() => {
      done = true
    })
    await vi.advanceTimersByTimeAsync(2999)
    expect(done).toBe(false)
    await vi.advanceTimersByTimeAsync(1)
    expect(done).toBe(true)
  })
})
