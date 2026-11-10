import { describe, expect, it } from "vitest"
import { NextRequest } from "next/server"

import { proxy } from "./proxy"

const csp = (path: string) =>
  proxy(new NextRequest(`https://flow.test${path}`)).headers.get(
    "Content-Security-Policy"
  )!

describe("Flow CSP", () => {
  it("lets only Flow itself frame the storefront preview, with no voice origins", () => {
    const policy = csp("/storefront/preview")
    expect(policy).toContain("frame-ancestors 'self'")
    expect(policy).not.toContain("frame-ancestors 'none'")
    expect(policy).not.toContain("elevenlabs.io")
  })

  it.each([
    "/",
    "/login",
    "/storefront",
    "/storefront/preview/x",
    "/storefront/previewx",
  ])("refuses framing on %s and keeps voice", (path) => {
    const policy = csp(path)
    expect(policy).toContain("frame-ancestors 'none'")
    expect(policy).toContain("wss://api.elevenlabs.io")
  })
})
