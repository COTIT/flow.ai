import { describe, expect, it } from "vitest"

import { isUuid, isPublicCode, normalizeCode } from "./passport"

describe("isPublicCode", () => {
  it("accepts 10 Crockford base32 characters", () => {
    expect(isPublicCode("7K3M9QX2WD")).toBe(true)
  })

  it.each([
    "",
    "7K3M9QX2W",
    "7K3M9QX2WDX",
    "7K3M9QX2WI",
    "../products",
    "7k3m9qx2wd/..",
  ])("refuses %j (so it can't reach another API path)", (code) =>
    expect(isPublicCode(code)).toBe(false)
  )
})

describe("isUuid", () => {
  it("accepts a UUID and nothing else", () => {
    expect(isUuid("11111111-1111-4111-8111-111111111111")).toBe(true)
    expect(isUuid("../lots/7K3M9QX2WD")).toBe(false)
    expect(isUuid("boom")).toBe(false)
  })
})

describe("normalizeCode", () => {
  it("reads a code typed in lower case with spaces and dashes", () => {
    expect(normalizeCode(" 7k3m 9qx2-wd ")).toBe("7K3M9QX2WD")
    expect(isPublicCode(normalizeCode("7k3m 9qx2-wd"))).toBe(true)
  })
  it("leaves nothing usable from junk", () => {
    expect(isPublicCode(normalizeCode("../etc"))).toBe(false)
  })
})
