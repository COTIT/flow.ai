import { describe, expect, it } from "vitest"

import { freshness } from "./freshness"

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
