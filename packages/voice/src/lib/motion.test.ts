import { describe, expect, it } from "vitest"
import { voiceMotionActive } from "./motion"

describe("decorative voice motion", () => {
  it("rests while quiet and stops for hidden, muted, text or disconnected sessions", () => {
    const active = {
      visible: true,
      connected: true,
      textOnly: false,
      muted: false,
      speaking: false,
      lastInputAt: 1000,
    }
    expect(voiceMotionActive(active, 2499)).toBe(true)
    expect(voiceMotionActive(active, 2500)).toBe(false)
    expect(voiceMotionActive({ ...active, speaking: true }, 3000)).toBe(true)
    for (const stop of [
      { visible: false },
      { connected: false },
      { textOnly: true },
      { muted: true },
    ])
      expect(
        voiceMotionActive({ ...active, speaking: true, ...stop }, 1001)
      ).toBe(false)
    expect(voiceMotionActive({ ...active, lastInputAt: null }, 1001)).toBe(
      false
    )
  })
})
