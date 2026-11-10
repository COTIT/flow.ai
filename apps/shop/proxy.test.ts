import { afterEach, describe, expect, it, vi } from "vitest"
import { NextRequest } from "next/server"

import { proxy } from "./proxy"
import en from "./messages/en.json"
import nl from "./messages/nl.json"
import sw from "./messages/sw.json"

afterEach(() => vi.unstubAllEnvs())

describe("unconfigured catalog", () => {
  it.each([undefined, "", " \t "])(
    "returns safe HTML503 for missing key %j",
    async (key) => {
      vi.stubEnv("STOREFRONT_APP_KEY", key)
      const response = proxy(new NextRequest("https://shop.test/"))
      expect(response.status).toBe(503)
      expect(response.headers.get("Content-Type")).toBe(
        "text/html; charset=utf-8"
      )
      expect(response.headers.get("Retry-After")).toBe("60")
      expect(response.headers.get("Cache-Control")).toBe("no-store")
      expect(response.headers.get("Content-Security-Policy")).toContain(
        "frame-ancestors 'none'"
      )
      expect(response.headers.get("Content-Security-Policy")).toContain(
        "'nonce-"
      )
      expect(response.headers.get("X-Content-Type-Options")).toBe("nosniff")
      expect(response.headers.get("Referrer-Policy")).toBe(
        "strict-origin-when-cross-origin"
      )
      expect(
        [...response.headers.keys()].some((name) =>
          name.startsWith("x-middleware")
        )
      ).toBe(false)
      const body = await response.text()
      expect(body).toContain("<main>")
      expect(body).toContain('<h1 id="unavailable">')
      expect(body).toContain(en.errors.unavailableTitle)
      expect(body).toContain('href="/contact"')
      expect(body).not.toMatch(
        /STOREFRONT_APP_KEY|gsk_|<script|https:\/\/shop\.test/
      )
    }
  )
  it.each([
    "/shop",
    "/checkout",
    "/products/item",
    "/orders/order",
    "/b/batch",
  ])("guards catalog route %s", (path) => {
    vi.stubEnv("STOREFRONT_APP_KEY", "")
    expect(proxy(new NextRequest(`https://shop.test${path}`)).status).toBe(503)
  })
  it.each([
    "/healthz",
    "/about",
    "/contact",
    "/faq",
    "/stories",
    "/suppliers",
    "/b",
    "/cart",
  ])("preserves passthrough for %s", (path) => {
    vi.stubEnv("STOREFRONT_APP_KEY", "")
    const response = proxy(new NextRequest(`https://shop.test${path}`))
    expect(response.headers.get("x-middleware-next")).toBe("1")
    expect(response.headers.get("Content-Security-Policy")).toBeTruthy()
  })
  it.each([
    ["en", en],
    ["nl", nl],
    ["sw", sw],
  ] as const)("uses existing %s unavailable copy", async (locale, messages) => {
    vi.stubEnv("STOREFRONT_APP_KEY", "")
    const response = proxy(
      new NextRequest("https://shop.test/", {
        headers: { cookie: `NEXT_LOCALE=${locale}` },
      })
    )
    const body = await response.text()
    expect(body).toContain(`<html lang="${locale}">`)
    expect(body).toContain(messages.errors.unavailableTitle)
    expect(body).toContain(
      messages.errors.unavailableBody.replaceAll("'", "&#39;")
    )
  })
  it("falls back to English for unknown locale cookies", async () => {
    vi.stubEnv("STOREFRONT_APP_KEY", "")
    const response = proxy(
      new NextRequest("https://shop.test/", {
        headers: { cookie: "NEXT_LOCALE=unknown" },
      })
    )
    const body = await response.text()
    expect(body).toContain('<html lang="en">')
    expect(body).toContain(en.errors.unavailableTitle)
  })
  it("never returns a HEAD body", async () => {
    vi.stubEnv("STOREFRONT_APP_KEY", "")
    const response = proxy(
      new NextRequest("https://shop.test/", {
        method: "HEAD",
        headers: { cookie: "NEXT_LOCALE=unknown" },
      })
    )
    expect(response.status).toBe(503)
    expect(await response.text()).toBe("")
  })
  it.each(["next-router-prefetch", "purpose"])(
    "guards missing-key prefetch %s while preserving configured behavior",
    (header) => {
      const request = new NextRequest("https://shop.test/", {
        headers: { [header]: "prefetch", rsc: "1" },
      })
      vi.stubEnv("STOREFRONT_APP_KEY", "")
      expect(proxy(request).status).toBe(503)
      vi.stubEnv("STOREFRONT_APP_KEY", "gsk_configured")
      const normal = proxy(request)
      expect(normal.headers.get("x-middleware-next")).toBe("1")
      expect(normal.headers.get("Content-Security-Policy")).toBeNull()
    }
  )
  it("preserves configured catalog requests", () => {
    vi.stubEnv("STOREFRONT_APP_KEY", "gsk_configured")
    const response = proxy(new NextRequest("https://shop.test/"))
    expect(response.headers.get("x-middleware-next")).toBe("1")
    expect(response.headers.get("Content-Security-Policy")).toBeTruthy()
  })
})
