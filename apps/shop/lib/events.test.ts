import { describe, expect, it } from "vitest"

import { EVENTS, upcoming } from "./events"

describe("upcoming", () => {
  it("keeps a market on its last day and drops it the day after", () => {
    const last = EVENTS.at(-1)!
    expect(upcoming(last.end)).toEqual([last])
    expect(upcoming("2026-12-21")).toEqual([])
  })
  it("keeps every market before the first one starts", () => {
    expect(upcoming("2026-01-01")).toHaveLength(EVENTS.length)
  })
})
