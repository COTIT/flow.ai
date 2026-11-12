import { describe, expect, it } from "vitest"

import { isLoadFailure } from "./load-failure"

const named = (name: string, message = "") => Object.assign(new Error(message), { name })

describe("isLoadFailure", () => {
  it("a page's code or data that never arrived is a load failure", () => {
    for (const error of [
      named("ChunkLoadError", "Loading chunk 742 failed."),
      new Error("Failed to load chunk /_next/static/chunks/app/(app)/storefront/page-1a2b.js"),
      new Error("Loading chunk 12 failed.\n(error: http://127.0.0.1/_next/static/chunks/12.js)"),
      new Error("Loading CSS chunk 3 failed."),
      new TypeError("Failed to fetch dynamically imported module: https://flow.genesy.test/x.js"),
      new TypeError("Importing a module script failed."),
      new Error("Failed to fetch RSC payload for https://flow.genesy.test/storefront."),
      new TypeError("Failed to fetch"),
      new TypeError("Load failed"),
      new TypeError("NetworkError when attempting to fetch resource."),
    ])
      expect(isLoadFailure(error), String(error)).toBe(true)
  })

  it("anything else is a crash, including odd values", () => {
    for (const error of [
      new TypeError("Cannot read properties of undefined (reading 'map')"),
      new Error("Minified React error #418"),
      new Error("This chunk of text failed validation"),
      new RangeError("Invalid time value"),
      null,
      undefined,
      "Loading chunk 1 failed.",
      { message: "Failed to fetch" },
    ])
      expect(isLoadFailure(error), String(error)).toBe(false)
  })
})
