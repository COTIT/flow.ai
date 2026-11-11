import { describe, expect, it } from "vitest"

import {
  comparePeriod,
  localToday,
  midnightIn,
  periodRange,
  probeRange,
  TTL,
} from "./scene-data"

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

describe("midnightIn (the instant a workspace-local day starts, as sales-summary takes it)", () => {
  it("is local midnight in the workspace zone, as an RFC 3339 UTC instant", () => {
    expect(midnightIn("2026-11-12", "Africa/Dar_es_Salaam")).toBe(
      "2026-11-11T21:00:00.000Z"
    )
    expect(midnightIn("2026-11-11", "America/Santiago")).toBe(
      "2026-11-11T03:00:00.000Z"
    )
  })
  it("follows daylight saving: the offset of that day, not of today", () => {
    expect(midnightIn("2026-07-01", "Europe/Berlin")).toBe(
      "2026-06-30T22:00:00.000Z"
    )
    expect(midnightIn("2026-12-01", "Europe/Berlin")).toBe(
      "2026-11-30T23:00:00.000Z"
    )
  })
  it("is UTC midnight when the zone is unknown or missing", () => {
    expect(midnightIn("2026-11-11", null)).toBe("2026-11-11T00:00:00.000Z")
    expect(midnightIn("2026-11-11", "Not/AZone")).toBe(
      "2026-11-11T00:00:00.000Z"
    )
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
    expect(periodRange("this_week", "2026-11-15")).toEqual({
      from: "2026-11-09",
      to: "2026-11-16",
    })
    expect(periodRange("this_week", "2026-11-09")).toEqual({
      from: "2026-11-09",
      to: "2026-11-10",
    })
  })
  it("crosses month and year ends", () => {
    expect(periodRange("last_7_days", "2026-01-03")).toEqual({
      from: "2025-12-28",
      to: "2026-01-04",
    })
    expect(periodRange("this_month", "2026-03-01")).toEqual({
      from: "2026-03-01",
      to: "2026-03-02",
    })
  })
})

describe("comparePeriod", () => {
  it("is the same span one week earlier for week periods", () => {
    expect(comparePeriod("this_week", "2026-11-11")).toEqual({
      from: "2026-11-02",
      to: "2026-11-05",
    })
    expect(comparePeriod("last_7_days", "2026-11-11")).toEqual({
      from: "2026-10-29",
      to: "2026-11-05",
    })
  })
  it("is the day before for a day period", () => {
    expect(comparePeriod("today", "2026-11-11")).toEqual({
      from: "2026-11-10",
      to: "2026-11-11",
    })
  })
  it("is the same days of last month for this month", () => {
    expect(comparePeriod("this_month", "2026-11-11")).toEqual({
      from: "2026-10-01",
      to: "2026-10-12",
    })
  })
})

describe("probeRange", () => {
  it("covers every zone's today around the UTC day", () => {
    expect(probeRange(new Date("2026-11-11T23:59:00Z"))).toEqual({
      from: "2026-11-10",
      to: "2026-11-13",
    })
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

describe("comparePeriod this_month never spills into the current month", () => {
  it("on 31 March compares 1-28 February (stops at 1 March)", () => {
    expect(comparePeriod("this_month", "2026-03-31")).toEqual({
      from: "2026-02-01",
      to: "2026-03-01",
    })
  })
  it("on 30 March in a leap year stops at 1 March", () => {
    expect(comparePeriod("this_month", "2028-03-30")).toEqual({
      from: "2028-02-01",
      to: "2028-03-01",
    })
  })
  it("on 29 February compares 1-29 January", () => {
    expect(comparePeriod("this_month", "2028-02-29")).toEqual({
      from: "2028-01-01",
      to: "2028-01-30",
    })
  })
})
