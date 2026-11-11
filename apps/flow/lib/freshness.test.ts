import { describe, expect, it } from "vitest"

import { ageLabel, freshness, nextTickDelay } from "./freshness"

const now = 1_000_000
describe("freshness", () => {
  it("is fresh with the age in seconds", () => {
    expect(
      freshness({
        updatedAt: now - 32_400,
        fetching: false,
        failed: false,
        now,
        staleAfter: 60_000,
      })
    ).toEqual({ kind: "fresh", seconds: 32 })
  })
  it("is refreshing while a refetch runs over shown data", () => {
    expect(
      freshness({
        updatedAt: now - 1000,
        fetching: true,
        failed: false,
        now,
        staleAfter: 60_000,
      }).kind
    ).toBe("refreshing")
  })
  it("is stale past twice its refresh time", () => {
    expect(
      freshness({
        updatedAt: now - 120_001,
        fetching: false,
        failed: false,
        now,
        staleAfter: 60_000,
      }).kind
    ).toBe("stale")
  })
  it("is failed when the last refresh failed over old data", () => {
    expect(
      freshness({
        updatedAt: now - 1000,
        fetching: false,
        failed: true,
        now,
        staleAfter: 60_000,
      }).kind
    ).toBe("failed")
  })
  it("has no label before the first answer", () => {
    expect(
      freshness({
        updatedAt: 0,
        fetching: true,
        failed: false,
        now,
        staleAfter: 60_000,
      }).kind
    ).toBe("loading")
  })
})

describe("ageLabel", () => {
  it.each([
    [0, { kind: "now" }],
    [59_999, { kind: "now" }],
    [60_000, { kind: "minutes", minutes: 1 }],
    [119_999, { kind: "minutes", minutes: 1 }],
    [120_000, { kind: "minutes", minutes: 2 }],
    [300_000, { kind: "minutes", minutes: 5 }],
    [3_599_999, { kind: "minutes", minutes: 59 }],
    [3_600_000, { kind: "at" }],
    [86_400_000, { kind: "at" }],
  ])("%i ms", (ms, label) => expect(ageLabel(ms)).toEqual(label))
})

describe("nextTickDelay", () => {
  it("is 30 s under 10 minutes and 60 s after", () => {
    expect(nextTickDelay(0)).toBe(30_000)
    expect(nextTickDelay(599_999)).toBe(30_000)
    expect(nextTickDelay(600_000)).toBe(60_000)
    expect(nextTickDelay(3_600_000)).toBe(60_000)
  })
})
