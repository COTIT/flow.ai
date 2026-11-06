import { describe, expect, it } from "vitest"

import {
  formatDate,
  formatDateTime,
  formatDay,
  formatQty,
  formatShare,
  formatTime,
} from "./format"

describe("formatShare", () => {
  it("shows basis points as a percentage, exactly", () => {
    expect(formatShare(10000, "en")).toBe("100%")
    expect(formatShare(2550, "en")).toBe("25.5%")
    expect(formatShare(5, "en")).toBe("0.05%")
    expect(formatShare(2550, "nl")).toBe("25,5%")
  })
})

describe("formatQty", () => {
  it("drops the decimals of a whole quantity and keeps them otherwise, exactly", () => {
    expect(formatQty("16.00", "en")).toBe("16")
    expect(formatQty("-1.00", "en")).toBe("-1")
    expect(formatQty("0.00", "en")).toBe("0")
    expect(formatQty("1.50", "en")).toBe("1.50")
    expect(formatQty("-0.25", "en")).toBe("-0.25")
    expect(formatQty("1.50", "nl")).toBe("1,50")
    expect(formatQty("1234.00", "en")).toBe("1,234")
    expect(formatQty("12345678901234.01", "en")).toBe("12,345,678,901,234.01")
  })
})

describe("formatDay", () => {
  it("shows an API date without shifting it by the server's time zone", () => {
    expect(formatDay("2026-07-14", "en")).toBe("July 14, 2026")
    expect(formatDay("2026-01-01", "nl")).toBe("1 januari 2026")
  })
})

describe("formatDate / formatDateTime / formatTime", () => {
  // Midday UTC: the same calendar day in every time zone the tests may run in.
  const at = "2026-11-04T12:12:49Z"

  it("shows a timestamp's day in words, never a machine date", () => {
    expect(formatDate(at, "en")).toBe("Nov 4, 2026")
    expect(formatDate(at, "nl")).toBe("4 nov 2026")
  })

  it("shows a plain API day without shifting it", () => {
    expect(formatDate("2026-01-01", "en")).toBe("Jan 1, 2026")
    expect(formatDate("2026-01-01", "sw")).not.toMatch(/\d{4}-\d{2}/)
  })

  it("adds the time in the locale's style", () => {
    expect(formatDateTime(at, "en")).toMatch(/^Nov 4, 2026, \d{1,2}:\d{2}\s[AP]M$/)
    expect(formatDateTime(at, "nl")).toMatch(/^4 nov 2026.* \d{2}:\d{2}$/)
    expect(formatTime(at, "nl")).toMatch(/^\d{2}:\d{2}$/)
  })
})
