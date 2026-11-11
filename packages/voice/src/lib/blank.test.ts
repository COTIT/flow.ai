import { describe, expect, it } from "vitest"

import { isBlankMessage } from "./blank"

describe("isBlankMessage", () => {
  it.each(["\u200b", "\u200c", "\u200d", "\u2060", "\ufeff"])(
    "a message of only U+%s is blank",
    (char) => {
      expect(isBlankMessage(char)).toBe(true)
      expect(isBlankMessage(char.repeat(5))).toBe(true)
    }
  )

  it("spaces, tabs, NBSP, U+3000 and zero-width marks mixed are blank", () => {
    expect(isBlankMessage("")).toBe(true)
    expect(isBlankMessage(" \t\n\u00a0\u3000\u200b\ufeff\u2060\u200d ")).toBe(
      true
    )
  })

  it("real text stays, a ZWJ emoji too", () => {
    expect(isBlankMessage("hi")).toBe(false)
    expect(isBlankMessage("\u200b ok \u200b")).toBe(false)
    expect(isBlankMessage("👩\u200d🌾")).toBe(false)
  })
})
