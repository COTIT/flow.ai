import { readdirSync, readFileSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"

import { contentSecurityPolicy, securityHeaders } from "./headers"

const directive = (csp: string, name: string) =>
  csp.split("; ").find((d) => d.startsWith(`${name} `))

describe("contentSecurityPolicy", () => {
  const csp = contentSecurityPolicy("abc123")

  it("runs only scripts carrying this response's nonce (and what they load)", () => {
    expect(directive(csp, "script-src")).toBe(
      "script-src 'self' 'nonce-abc123' 'strict-dynamic'"
    )
  })

  it("never allows inline or eval'd script in production", () => {
    expect(directive(csp, "script-src")).not.toMatch(
      /unsafe-inline|unsafe-eval/
    )
  })

  it("refuses framing, plugins, base-tag and form hijacks", () => {
    expect(csp).toContain("frame-ancestors 'none'")
    expect(csp).toContain("object-src 'none'")
    expect(csp).toContain("base-uri 'self'")
    expect(csp).toContain("form-action 'self'")
    expect(csp).toContain("default-src 'self'")
  })

  it("opens the dev reload websocket only in development", () => {
    expect(directive(csp, "connect-src")).toBe("connect-src 'self'")
    expect(
      directive(contentSecurityPolicy("n", { dev: true }), "connect-src")
    ).toBe("connect-src 'self' ws:")
  })

  it("lets a page that must be embedded name who may frame it", () => {
    const shop = contentSecurityPolicy("n", {
      frameAncestors: "https://partner.example",
    })
    expect(shop).toContain("frame-ancestors https://partner.example")
  })

  it("lets images load from one exact file-storage origin", () => {
    expect(
      directive(
        contentSecurityPolicy("n", { imgSrc: "https://s3.eu.example.com" }),
        "img-src"
      )
    ).toBe("img-src 'self' blob: data: https://s3.eu.example.com")
  })

  it.each([
    undefined,
    "",
    "https://*.example.com",
    "*",
    "https://files.example.com/bucket",
    "https://a.example.com https://b.example.com",
    "'unsafe-inline'",
    "javascript:alert(1)",
    "ftp://files.example.com",
  ])("adds nothing to img-src for %j", (imgSrc) => {
    expect(directive(contentSecurityPolicy("n", { imgSrc }), "img-src")).toBe(
      "img-src 'self' blob: data:"
    )
  })

  it("allows eval only in development, for React's dev tooling", () => {
    expect(
      directive(contentSecurityPolicy("n", { dev: true }), "script-src")
    ).toContain("'unsafe-eval'")
  })
})

describe("securityHeaders", () => {
  it("stops MIME sniffing and full-URL referrers", () => {
    expect(securityHeaders).toEqual(
      expect.arrayContaining([
        { key: "X-Content-Type-Options", value: "nosniff" },
        { key: "Referrer-Policy", value: "strict-origin-when-cross-origin" },
      ])
    )
  })
})

// Build check: every Next app wires the shared proxy and headers.
const apps = join(import.meta.dirname, "../../../apps")
const nextApps = readdirSync(apps).filter((a) => {
  try {
    return readFileSync(join(apps, a, "package.json"), "utf8").includes(
      '"next"'
    )
  } catch {
    return false
  }
})

describe.each(nextApps)("app %s", (app) => {
  it("sends the CSP from proxy.ts", () => {
    expect(readFileSync(join(apps, app, "proxy.ts"), "utf8")).toContain(
      "@genesy/security/proxy"
    )
  })

  it("sends the static security headers from next.config.ts", () => {
    const config = readFileSync(join(apps, app, "next.config.ts"), "utf8")
    expect(config).toContain("@genesy/security/headers")
    expect(config).toMatch(/headers:\s*async/)
  })
})
