import { describe, expect, it } from "vitest"

import { comparePeriod, localToday, periodRange, probeRange, TTL } from "./scene-data"

describe("localToday", () => {
  const instant = new Date("2026-11-11T22:30:00Z")
  it("is the workspace's calendar day, not the browser's", () => {
    expect(localToday(instant, "Africa/Dar_es_Salaam")).toBe("2026-11-12")
    expect(localToday(instant, "America/Santiago")).toBe("2026-11-11")
  })
  it("falls back to UTC when the zone is unknown or missing", () => {
    expect(localToday(instant, null)).toBe("2026-11-11")
    expect(localToday(instant, "Not/AZone")).toBe("2026-11-11")
  })
})

describe("periodRange (from inclusive, to exclusive)", () => {
  // 2026-11-11 is a Wednesday.
  const today = "2026-11-11"
  it.each([
    ["today", "2026-11-11", "2026-11-12"],
    ["yesterday", "2026-11-10", "2026-11-11"],
    ["this_week", "2026-11-09", "2026-11-12"],
    ["last_week", "2026-11-02", "2026-11-09"],
    ["last_7_days", "2026-11-05", "2026-11-12"],
    ["this_month", "2026-11-01", "2026-11-12"],
  ] as const)("%s", (period, from, to) => {
    expect(periodRange(period, today)).toEqual({ from, to })
  })
  it("starts the week on Monday, also on a Sunday and a Monday", () => {
    expect(periodRange("this_week", "2026-11-15")).toEqual({ from: "2026-11-09", to: "2026-11-16" })
    expect(periodRange("this_week", "2026-11-09")).toEqual({ from: "2026-11-09", to: "2026-11-10" })
  })
  it("crosses month and year ends", () => {
    expect(periodRange("last_7_days", "2026-01-03")).toEqual({ from: "2025-12-28", to: "2026-01-04" })
    expect(periodRange("this_month", "2026-03-01")).toEqual({ from: "2026-03-01", to: "2026-03-02" })
  })
})

describe("comparePeriod", () => {
  it("is the same span one week earlier for week periods", () => {
    expect(comparePeriod("this_week", "2026-11-11")).toEqual({ from: "2026-11-02", to: "2026-11-05" })
    expect(comparePeriod("last_7_days", "2026-11-11")).toEqual({ from: "2026-10-29", to: "2026-11-05" })
  })
  it("is the day before for a day period", () => {
    expect(comparePeriod("today", "2026-11-11")).toEqual({ from: "2026-11-10", to: "2026-11-11" })
  })
  it("is the same days of last month for this month", () => {
    expect(comparePeriod("this_month", "2026-11-11")).toEqual({ from: "2026-10-01", to: "2026-10-12" })
  })
})

describe("probeRange", () => {
  it("covers every zone's today around the UTC day", () => {
    expect(probeRange(new Date("2026-11-11T23:59:00Z"))).toEqual({ from: "2026-11-10", to: "2026-11-13" })
  })
})

describe("TTL", () => {
  it("refreshes pending work faster than history", () => {
    expect(TTL.orders).toBe(30_000)
    expect(TTL.attention).toBe(60_000)
    expect(TTL.sales).toBe(120_000)
    expect(TTL.series).toBe(300_000)
  })
})
