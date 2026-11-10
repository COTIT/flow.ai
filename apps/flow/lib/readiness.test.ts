import { describe, expect, it } from "vitest"

import { readinessState } from "./readiness"

describe("readinessState", () => {
  it("200 ready is available", () => {
    expect(readinessState(200, { enabled: true, ready: true })).toBe(
      "available"
    )
  })
  it("503 with the assistant switched off is off", () => {
    expect(readinessState(503, { enabled: false, ready: false })).toBe("off")
  })
  it("503 while switched on is not ready", () => {
    expect(readinessState(503, { enabled: true, ready: false })).toBe(
      "not_ready"
    )
  })
  it("proxy failures and odd answers are unknown", () => {
    expect(readinessState(502, { message: "x" })).toBe("unknown")
    expect(readinessState(504, undefined)).toBe("unknown")
    expect(readinessState(200, { enabled: true, ready: false })).toBe("unknown")
    expect(readinessState(503, null)).toBe("unknown")
  })
})
