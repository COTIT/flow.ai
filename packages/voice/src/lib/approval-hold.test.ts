import { describe, expect, it } from "vitest"

import { approvalBlocked, approvalHeld } from "./approval-hold"

describe("approval hold", () => {
  it("never blocks when no host holds anything (Office)", () => {
    expect(approvalBlocked(null, "approve", "t1")).toBe(false)
    expect(approvalHeld(null, "t1")).toBe(false)
  })

  it("blocks approving the held review, by button or by voice (both call confirm)", () => {
    expect(approvalBlocked("t1", "approve", "t1")).toBe(true)
    expect(approvalHeld("t1", "t1")).toBe(true)
  })

  it("still lets the held review be cancelled", () => {
    expect(approvalBlocked("t1", "cancel", "t1")).toBe(false)
  })

  it("clears for a new review: a hold names one ticket", () => {
    expect(approvalBlocked("t1", "approve", "t2")).toBe(false)
    expect(approvalHeld("t1", "t2")).toBe(false)
    expect(approvalHeld("t1", undefined)).toBe(false)
  })
})
