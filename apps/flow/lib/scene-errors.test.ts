import { describe, expect, it } from "vitest"

import { retryDelay, sceneError, shouldRetry } from "./scene-errors"

const http = (status: number, headers: Record<string, string> = {}) => ({
  response: { status, headers },
})

describe("sceneError", () => {
  it.each([
    [401, "unauthorized"],
    [403, "forbidden"],
    [404, "missing"],
    [429, "busy"],
    [408, "transient"],
    [422, "other"],
    [500, "transient"],
    [503, "transient"],
  ])("%i is %s", (status, kind) =>
    expect(sceneError(http(status)).kind).toBe(kind)
  )
  it("treats no answer (network) as transient", () => {
    expect(sceneError(new Error("Network Error")).kind).toBe("transient")
    expect(sceneError(null).kind).toBe("transient")
  })
  it("reads Retry-After seconds, default 5, capped at 60", () => {
    expect(sceneError(http(429, { "retry-after": "12" }))).toEqual({
      kind: "busy",
      retryAfter: 12,
    })
    expect(sceneError(http(429))).toEqual({ kind: "busy", retryAfter: 5 })
    expect(sceneError(http(429, { "retry-after": "999" }))).toEqual({
      kind: "busy",
      retryAfter: 60,
    })
    expect(sceneError(http(429, { "retry-after": "soon" }))).toEqual({
      kind: "busy",
      retryAfter: 5,
    })
  })
})

describe("shouldRetry", () => {
  it("never retries 401, 403, 404 or other 4xx", () => {
    for (const s of [400, 401, 403, 404, 409, 422])
      expect(shouldRetry(0, http(s))).toBe(false)
  })
  it("retries 408, 429, 5xx and network errors, at most 3 times", () => {
    for (const e of [http(408), http(429), http(500), new Error("x")]) {
      expect(shouldRetry(0, e)).toBe(true)
      expect(shouldRetry(2, e)).toBe(true)
      expect(shouldRetry(3, e)).toBe(false)
    }
  })
})

describe("retryDelay", () => {
  it("backs off 1 s, 2 s, 4 s for 5xx, capped at 10 s", () => {
    expect(retryDelay(0, http(500))).toBe(1000)
    expect(retryDelay(1, http(500))).toBe(2000)
    expect(retryDelay(2, http(500))).toBe(4000)
    expect(retryDelay(9, http(500))).toBe(10_000)
  })
  it("waits what Retry-After says for 429", () => {
    expect(retryDelay(0, http(429, { "retry-after": "7" }))).toBe(7000)
  })
})
