import { describe, expect, it, vi } from "vitest"
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

describe("Flow CSP images", () => {
  it.each(["/", "/storefront/preview"])(
    "allows the signed file host (FILES_ORIGIN) as an image source on %s, like Office",
    async (path) => {
      vi.stubEnv("FILES_ORIGIN", "https://files.example")
      vi.resetModules()
      const fresh = (await import("./proxy")).proxy
      const policy = fresh(
        new NextRequest(`https://flow.test${path}`)
      ).headers.get("Content-Security-Policy")!
      expect(policy).toMatch(/img-src [^;]*https:\/\/files\.example/)
      vi.unstubAllEnvs()
    }
  )
})
