import { describe, expect, it } from "vitest"

import { formatDay, formatShare } from "./format"

describe("formatShare", () => {
  it("shows basis points as a percentage, exactly", () => {
    expect(formatShare(10000, "en")).toBe("100%")
    expect(formatShare(2550, "en")).toBe("25.5%")
    expect(formatShare(5, "en")).toBe("0.05%")
    expect(formatShare(2550, "nl")).toBe("25,5%")
  })
})

describe("formatDay", () => {
  it("shows an API date without shifting it by the server's time zone", () => {
    expect(formatDay("2026-07-14", "en")).toBe("July 14, 2026")
    expect(formatDay("2026-01-01", "nl")).toBe("1 januari 2026")
  })
})
