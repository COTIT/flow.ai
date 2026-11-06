import { describe, expect, it } from "vitest"

import { initials } from "./story-card"

describe("initials", () => {
  it("takes the capitals and skips a nickname", () => {
    expect(initials('Benard Omondi, "Art Benny"')).toBe("BO")
    expect(initials("The Illundo bee farmers")).toBe("TI")
    expect(initials("Mary")).toBe("M")
  })
})
