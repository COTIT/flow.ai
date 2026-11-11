import { beforeEach, describe, expect, it, vi } from "vitest"

// vitest runs in node: an in-memory localStorage.
const saved = new Map<string, string>()
vi.stubGlobal("localStorage", {
  getItem: (k: string) => saved.get(k) ?? null,
  setItem: (k: string, v: string) => saved.set(k, v),
  removeItem: (k: string) => saved.delete(k),
})

import {
  activityKey,
  dismissHint,
  readActivity,
  recordUse,
  resetActivity,
} from "./activity"

const DAY = 86_400_000
const key = activityKey("u1", "w1")

beforeEach(() => saved.clear())

describe("activity (this device only)", () => {
  it("counts uses per person and workspace", () => {
    recordUse(key, "review_low_stock", 10)
    recordUse(key, "review_low_stock", 20)
    expect(readActivity(key, 30).uses).toEqual({
      review_low_stock: { n: 2, last: 20 },
    })
    expect(readActivity(activityKey("u1", "w2"), 30).uses).toEqual({})
    expect(readActivity(activityKey("u2", "w1"), 30).uses).toEqual({})
  })

  it("ignores ids that are not capabilities", () => {
    recordUse(key, "made_up", 10)
    saved.set(key, JSON.stringify({ v: 1, uses: { nope: { n: 9, last: 1 } } }))
    expect(readActivity(key, 30).uses).toEqual({})
  })

  it("forgets uses older than 90 days", () => {
    recordUse(key, "pending_orders", 0)
    recordUse(key, "explore", 50 * DAY)
    expect(Object.keys(readActivity(key, 91 * DAY).uses)).toEqual(["explore"])
  })

  it("survives corrupt or missing storage", () => {
    saved.set(key, "{not json")
    expect(readActivity(key, 1)).toEqual({ uses: {}, hintDismissed: false })
    vi.stubGlobal("localStorage", undefined)
    expect(readActivity(key, 1)).toEqual({ uses: {}, hintDismissed: false })
    expect(() => recordUse(key, "explore", 1)).not.toThrow()
    vi.stubGlobal("localStorage", {
      getItem: (k: string) => saved.get(k) ?? null,
      setItem: (k: string, v: string) => saved.set(k, v),
      removeItem: (k: string) => saved.delete(k),
    })
  })

  it("remembers a dismissed hint; reset clears the counts only", () => {
    recordUse(key, "explore", 1)
    dismissHint(key)
    expect(readActivity(key, 2).hintDismissed).toBe(true)
    resetActivity(key)
    expect(readActivity(key, 2)).toEqual({ uses: {}, hintDismissed: true })
  })
})
