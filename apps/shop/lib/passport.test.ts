import { describe, expect, it } from "vitest"

import { earliestLinkExpiry, isUuid, isPublicCode } from "./passport"

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

describe("earliestLinkExpiry", () => {
  const link = (expires_at: string) => ({ url: "https://f/x", expires_at })
  const lot = {
    passport: {
      banner: link("2026-10-31T10:10:00Z"),
      certificates: [{ file: link("2026-10-31T10:05:00Z") }, { file: null }],
      story: [
        {
          kind: "image" as const,
          image: link("2026-10-31T10:08:00Z"),
          caption: null,
        },
        { kind: "quote" as const, text: "hi", by: null },
      ],
    },
  }

  it("finds the first signed link to expire", () => {
    expect(earliestLinkExpiry(lot)?.toISOString()).toBe(
      "2026-10-31T10:05:00.000Z"
    )
  })

  it("is null when the page has no links", () => {
    expect(
      earliestLinkExpiry({
        passport: { banner: null, certificates: [], story: [] },
      })
    ).toBeNull()
  })
})
