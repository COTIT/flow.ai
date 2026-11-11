import { describe, expect, it } from "vitest"

import { approvalBlocked, approvalHeld } from "./approval-hold"

const me = "w1:token-a"
const hold = { ticket: "t1", identity: me }

describe("approval hold", () => {
  it("never blocks when no host holds anything (Office)", () => {
    expect(approvalBlocked(null, "approve", "t1", me)).toBe(false)
    expect(approvalHeld(null, "t1", me)).toBe(false)
  })

  it("blocks approving the held review, by button or by voice (both call confirm)", () => {
    expect(approvalBlocked(hold, "approve", "t1", me)).toBe(true)
    expect(approvalHeld(hold, "t1", me)).toBe(true)
  })

  it("still lets the held review be cancelled", () => {
    expect(approvalBlocked(hold, "cancel", "t1", me)).toBe(false)
  })

  it("clears for a new review: a hold names one ticket", () => {
    expect(approvalBlocked(hold, "approve", "t2", me)).toBe(false)
    expect(approvalHeld(hold, "t2", me)).toBe(false)
    expect(approvalHeld(hold, undefined, me)).toBe(false)
  })

  it("never outlives the login or workspace it was set under", () => {
    expect(approvalHeld(hold, "t1", "w2:token-a")).toBe(false)
    expect(approvalHeld(hold, "t1", "w1:token-b")).toBe(false)
    expect(approvalBlocked(hold, "approve", "t1", "w1:token-b")).toBe(false)
  })
})
