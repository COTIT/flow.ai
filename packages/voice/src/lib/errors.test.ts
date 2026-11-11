import { describe, expect, it } from "vitest"

import { VoiceRequestError, VoiceShownError, voiceErrorMessage } from "./errors"

// Echoes the key so a test reads which text the person sees.
const t = (key: string) => `<${key}>`

describe("voiceErrorMessage", () => {
  it("never shows the browser's own text for a dropped or garbled request", () => {
    expect(voiceErrorMessage(new TypeError("Failed to fetch"), t)).toBe(
      "<requestFailed>"
    )
    expect(
      voiceErrorMessage(
        new SyntaxError(`Unexpected token '<', "<html>" is not valid JSON`),
        t
      )
    ).toBe("<requestFailed>")
    expect(voiceErrorMessage("boom", t)).toBe("<requestFailed>")
    expect(
      voiceErrorMessage(
        new Error("could not establish pc connection"),
        t,
        "unknown"
      )
    ).toBe("<unknown>")
  })

  it("a read that lost the line says so neutrally", () => {
    for (const kind of ["network", "bad_response"] as const)
      expect(
        voiceErrorMessage(new VoiceRequestError(kind, 0, undefined, kind), t)
      ).toBe("<connectionLost>")
  })

  it("a write that lost the line asks to check before repeating", () => {
    expect(
      voiceErrorMessage(
        new VoiceRequestError("network", 0, undefined, "network"),
        t,
        "requestFailed",
        "requestErrors.unverified"
      )
    ).toBe("<requestErrors.unverified>")
  })

  it("offline says offline, read or write", () => {
    const offline = new VoiceRequestError("offline", 0, undefined, "offline")
    expect(voiceErrorMessage(offline, t)).toBe("<offline>")
    expect(
      voiceErrorMessage(offline, t, "requestFailed", "requestErrors.unverified")
    ).toBe("<offline>")
  })

  it("keeps text Genesy already translated", () => {
    expect(
      voiceErrorMessage(new VoiceRequestError("Your session expired.", 401), t)
    ).toBe("Your session expired.")
    expect(
      voiceErrorMessage(new VoiceShownError("Check Office first."), t)
    ).toBe("Check Office first.")
  })

  it("names a refused or missing microphone in plain words", () => {
    expect(
      voiceErrorMessage(new DOMException("denied", "NotAllowedError"), t)
    ).toBe("<microphoneDenied>")
    expect(
      voiceErrorMessage(
        new DOMException("Permission denied", "SecurityError"),
        t
      )
    ).toBe("<microphoneDenied>")
    expect(
      voiceErrorMessage(new DOMException("none", "NotFoundError"), t)
    ).toBe("<microphoneMissing>")
    expect(
      voiceErrorMessage(new DOMException("none", "OverconstrainedError"), t)
    ).toBe("<microphoneMissing>")
  })
})
