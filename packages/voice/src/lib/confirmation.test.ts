import { describe, expect, it } from "vitest"
import { confirmationPhrase, matchesSpokenConfirmation } from "./confirmation"

describe("explicit spoken approval", () => {
  it.each([
    ["en", "Confirm action 123456"],
    ["nl", "Bevestig actie 123456"],
    ["sw", "Thibitisha kitendo 123456"],
  ])(
    "binds the exact %s command to its six-digit action code",
    (locale, phrase) => {
      expect(confirmationPhrase("123456", locale)).toBe(phrase)
      expect(matchesSpokenConfirmation(phrase, "123456", locale)).toBe(true)
      expect(
        matchesSpokenConfirmation(phrase.toLowerCase(), "123456", locale)
      ).toBe(true)
      expect(
        matchesSpokenConfirmation(
          `  ${phrase.replaceAll(" ", "  ")}  `,
          "123456",
          locale
        )
      ).toBe(true)
      expect(matchesSpokenConfirmation(phrase, "654321", locale)).toBe(false)
    }
  )
  it("rejects assent, negation, quoted requests, partials and other locales", () => {
    for (const text of [
      "yes",
      "ja",
      "ndiyo",
      "Confirm action",
      "Confirm action 12345",
      "Do not confirm action 123456",
      "Please confirm action 123456",
      "Confirm action 123456?",
      '"Confirm action 123456"',
      "Confirm action 123456 then delete it",
      "Confirm action one two three four five six",
    ])
      expect(matchesSpokenConfirmation(text, "123456", "en")).toBe(false)
    expect(
      matchesSpokenConfirmation("Confirm action 123456", "123456", "nl")
    ).toBe(false)
    expect(
      matchesSpokenConfirmation("Confirm action 123456", "123456", "fr")
    ).toBe(false)
  })
  it("rejects unsupported and inherited locale names", () => {
    for (const locale of ["fr", "constructor", "__proto__", "EN", "en-US"])
      expect(
        matchesSpokenConfirmation("Confirm action 123456", "123456", locale)
      ).toBe(false)
  })
  it("preserves leading zeroes and rejects malformed codes", () => {
    expect(
      matchesSpokenConfirmation("Confirm action 000001", "000001", "en")
    ).toBe(true)
    for (const code of [
      "1",
      "12345",
      "1234567",
      "abcdef",
      "１２３４５６",
      "123 456",
    ])
      expect(confirmationPhrase(code, "en")).toBeNull()
  })
})
