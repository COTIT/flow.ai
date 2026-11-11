import { expect, it } from "vitest"
import { NextRequest, type NextFetchEvent } from "next/server"
import { cspProxy } from "./proxy"

it("accepts Next's event argument when exported directly as an app proxy", () => {
  const event = { waitUntil: () => {} } as unknown as NextFetchEvent
  const response = cspProxy()(new NextRequest("https://landing.test/"), event)
  expect(response.headers.get("x-middleware-next")).toBe("1")
  expect(response.headers.get("x-middleware-rewrite")).toBeNull()
  expect(response.headers.get("Content-Security-Policy")).toContain("'nonce-")
})

it("supports persistent Office voice from any initial document route", () => {
  const response = cspProxy({ voice: true })(new NextRequest("https://office.test/orders"))
  expect(response.headers.get("Content-Security-Policy")).toContain("wss://api.elevenlabs.io")
})

it("keeps other applications' default policy free from voice origins", () => {
  const response = cspProxy()(new NextRequest("https://shop.test/products"))
  expect(response.headers.get("Content-Security-Policy")).not.toContain("elevenlabs.io")
})
