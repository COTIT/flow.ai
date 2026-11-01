import { describe, expect, it } from "vitest"

import { formatMoney, toMinor } from "./money"

// Intl puts a narrow no-break space in some locales; compare on plain spaces.
const fmt = (...args: Parameters<typeof formatMoney>) =>
  formatMoney(...args).replace(/\s/g, " ")

describe("formatMoney", () => {
  it("shows minor units with the currency's decimals", () => {
    expect(fmt(1250, 2, "USD", "en")).toBe("$12.50")
    expect(fmt(5, 2, "USD", "en")).toBe("$0.05")
    expect(fmt(0, 2, "EUR", "nl")).toBe("€ 0,00")
  })

  it("keeps currencies with no or three decimals exact", () => {
    expect(fmt(125000, 0, "TZS", "en")).toBe("TZS 125,000")
    expect(fmt(1234, 3, "KWD", "en")).toBe("KWD 1.234")
  })

  it("handles negative amounts (refunds)", () => {
    expect(fmt(-1250, 2, "USD", "en")).toBe("-$12.50")
    expect(fmt(-5, 2, "USD", "en")).toBe("-$0.05")
  })

  it("never goes through a float, even past 2^53 minor units", () => {
    expect(fmt(9007199254740993n, 2, "USD", "en")).toBe(
      "$90,071,992,547,409.93"
    )
  })
})

describe("toMinor", () => {
  it("turns a typed amount into minor units by string", () => {
    expect(toMinor("12.5", 2)).toBe(1250)
    expect(toMinor("0,05", 2)).toBe(5)
    expect(toMinor(" 20 ", 2)).toBe(2000)
    expect(toMinor("500", 0)).toBe(500)
  })

  it("refuses what isn't a plain positive amount", () => {
    expect(toMinor("", 2)).toBeNull()
    expect(toMinor("0", 2)).toBeNull()
    expect(toMinor("-1", 2)).toBeNull()
    expect(toMinor("1.005", 2)).toBeNull()
    expect(toMinor("1e3", 2)).toBeNull()
    expect(toMinor("99999999999999999", 2)).toBeNull()
  })
})
