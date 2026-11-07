import { expect, it } from "vitest"
import { NextRequest } from "next/server"
import { cspProxy } from "./proxy"

it("supports persistent Office voice from any initial document route", () => {
  const response = cspProxy({ voice: true })(new NextRequest("https://office.test/orders"))
  expect(response.headers.get("Content-Security-Policy")).toContain("wss://api.elevenlabs.io")
})

it("keeps other applications' default policy free from voice origins", () => {
  const response = cspProxy()(new NextRequest("https://shop.test/products"))
  expect(response.headers.get("Content-Security-Policy")).not.toContain("elevenlabs.io")
})
