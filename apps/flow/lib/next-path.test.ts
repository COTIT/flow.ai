import { describe, expect, it } from "vitest"

import { safeNext } from "./next-path"

const origin = "https://flow.genesy.test"

describe("safeNext", () => {
  it("keeps a same-origin path with its query and hash", () => {
    expect(safeNext("/scenes/pulse", origin)).toBe("/scenes/pulse")
    expect(safeNext("/scenes/sales?period=month#top", origin)).toBe(
      "/scenes/sales?period=month#top"
    )
  })

  it("sends anything else home", () => {
    for (const next of [
      null,
      undefined,
      "",
      "scenes/pulse",
      "//evil.test",
      "//evil.test/scenes",
      "/\\evil.test",
      "\\/evil.test",
      "/\\/evil.test",
      "https://evil.test/",
      "javascript:alert(1)",
      "/\tevil",
      "/\n/evil.test",
      "/\r\n//evil.test",
      "/\u0000x",
      "/\u007fx",
      " /scenes/pulse",
      "/login",
      "/login?next=/scenes",
    ])
      expect(safeNext(next, origin), String(next)).toBe("/")
  })
})
