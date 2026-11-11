import { describe, expect, it } from "vitest"
import type { ResultView } from "@genesy/voice/lib/views"
import { present, viewChoice } from "./present"
import { VoiceError } from "./server"

const ROW = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b"
const daily: ResultView = {
  kind: "sales_daily",
  from: "2026-11-01",
  to: "2026-11-03",
  time_zone: "UTC",
  currencies: [{ currency: "USD", decimals: 2, days: [{ date: "2026-11-01", gross: 5, net: 5, orders: 1 }] }],
}
const read = { status: "completed", data: { kept: true }, view: daily }

describe("the agent's view choice", () => {
  it("is refused on a write or a secure form: those keep their review", () => {
    for (const input of [{ presentation: "pie" }, { replaces: ROW }])
      expect(() => viewChoice(input, { write: true, disposition: "execute" })).toThrow(VoiceError)
    expect(() => viewChoice({ presentation: "bar" }, { write: false, disposition: "secure_ui" })).toThrow(VoiceError)
    expect(viewChoice({}, { write: true, disposition: "execute" })).toBeUndefined()
  })

  it("keeps only a well-formed picture id to replace", () => {
    expect(viewChoice({ presentation: "bar", replaces: ROW.toUpperCase() }, { write: false, disposition: "execute" })).toEqual({
      presentation: "bar",
      replaces: ROW,
    })
    expect(viewChoice({ replaces: "../x" }, { write: false, disposition: "execute" })).toEqual({ presentation: undefined, replaces: undefined })
  })
})

describe("present", () => {
  it("draws a supported choice and keeps the data untouched", () => {
    const out = present(read, { presentation: "bar", replaces: ROW })
    expect(out).toMatchObject({ data: { kept: true }, view: { ...daily, presentation: "bar" }, replaces: ROW })
    expect(out).not.toHaveProperty("presentation_note")
  })

  it("falls back to the default and says why for an unsupported choice", () => {
    expect(present(read, { presentation: "pie" })).toMatchObject({
      view: daily,
      presentation_note: { asked: "pie", shown: "line", closest: "bar", reason: expect.stringMatching(/parts of one whole/) },
    })
  })

  it("leaves outcomes without a picture alone", () => {
    const failed = { status: "failed", message: "x" }
    expect(present(failed, { presentation: "pie", replaces: ROW })).toBe(failed)
    expect(present(read, undefined)).toBe(read)
  })
})
