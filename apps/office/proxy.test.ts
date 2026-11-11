import { unstable_doesMiddlewareMatch as runs } from "next/experimental/testing/server"
import { expect, it } from "vitest"
import { config } from "./proxy"

const at = (path: string, headers?: Record<string, string>) =>
  runs({ config, url: `http://office.test${path}`, headers })

it("covers page navigations (nonce CSP + voice gap follow-up)", () => {
  expect(at("/login")).toBe(true)
  expect(at("/orders/0195-a")).toBe(true)
  expect(at("/")).toBe(true)
})

it("skips route handlers: Next cuts any body the proxy sees at 10 MiB, voice uploads go up to 10 MiB + 64 KiB", () => {
  expect(at("/api/voice/attachments")).toBe(false)
  expect(at("/api/voice/tools")).toBe(false)
  expect(at("/api/voice/session")).toBe(false)
})

it("still skips the /v1 backend forward, static files and prefetches", () => {
  expect(at("/v1/orders")).toBe(false)
  expect(at("/_next/static/chunk.js")).toBe(false)
  expect(at("/orders", { "next-router-prefetch": "1" })).toBe(false)
  expect(at("/orders", { purpose: "prefetch" })).toBe(false)
})
