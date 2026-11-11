import { afterEach, describe, expect, it, vi } from "vitest"

import { INTENT_LIMIT_MS, intentPending, type IntentNow, type SentIntent } from "./once-intent"

afterEach(() => vi.useRealTimers())

const row = (id: string, role: IntentNow["rows"][number]["role"]) => ({ id, role })
const sent = (over: Partial<SentIntent> = {}): SentIntent => ({
  key: "b-1",
  after: "r1",
  error: "",
  at: Date.now(),
  ...over,
})
const now = (over: Partial<IntentNow> = {}): IntentNow => ({
  key: "b-1",
  rows: [row("r1", "agent")],
  error: "",
  connected: true,
  ...over,
})

describe("intentPending", () => {
  it("nothing sent: not pending", () => {
    expect(intentPending(null, now())).toBe(false)
  })

  it("sent and not answered yet: pending", () => {
    expect(intentPending(sent(), now())).toBe(true)
  })

  it("its own user line is not an answer", () => {
    expect(intentPending(sent(), now({ rows: [row("r1", "agent"), row("u1", "user")] }))).toBe(true)
  })

  it("an agent line after the send answers it", () => {
    const rows = [row("r1", "agent"), row("u1", "user"), row("a1", "agent")]
    expect(intentPending(sent(), now({ rows }))).toBe(false)
  })

  it("a result or action after the send answers it too", () => {
    for (const role of ["result", "action", "link"] as const)
      expect(intentPending(sent(), now({ rows: [row("r1", "agent"), row("x", role)] }))).toBe(false)
  })

  it("rows from before the send never count as the answer", () => {
    const rows = [row("a0", "agent"), row("r1", "user")]
    expect(intentPending(sent(), now({ rows }))).toBe(true)
  })

  it("sent on an empty transcript: the first agent line answers it", () => {
    expect(intentPending(sent({ after: undefined }), now({ rows: [row("u1", "user")] }))).toBe(true)
    expect(
      intentPending(sent({ after: undefined }), now({ rows: [row("u1", "user"), row("a1", "agent")] }))
    ).toBe(false)
  })

  it("the send point scrolled out of the capped transcript: long since answered", () => {
    expect(intentPending(sent({ after: "gone" }), now({ rows: [row("u9", "user")] }))).toBe(false)
  })

  it("a new error ends it; the error it was sent with does not", () => {
    expect(intentPending(sent(), now({ error: "Request failed" }))).toBe(false)
    expect(intentPending(sent({ error: "old" }), now({ error: "old" }))).toBe(true)
  })

  it("a dropped connection ends it", () => {
    expect(intentPending(sent(), now({ connected: false }))).toBe(false)
  })

  it("another subject (new batch, review or question) ends it", () => {
    expect(intentPending(sent(), now({ key: "b-2" }))).toBe(false)
  })

  it("an answer that never comes stops blocking after the limit", () => {
    vi.useFakeTimers()
    const intent = sent()
    vi.advanceTimersByTime(INTENT_LIMIT_MS - 1)
    expect(intentPending(intent, now())).toBe(true)
    vi.advanceTimersByTime(1)
    expect(intentPending(intent, now())).toBe(false)
  })
})
