import { readdirSync, readFileSync } from "node:fs"
import { describe, expect, it } from "vitest"

// lib/* is imported by Office server routes and client components alike, so it stays plain TS.
const dir = new URL("./lib/", import.meta.url)
const sources = readdirSync(dir).filter((f) => f.endsWith(".ts") && !f.endsWith(".test.ts"))

describe("src/lib stays pure", () => {
  it.each(sources)("%s has no client, server, Node, Next or component import", (file) => {
    const code = readFileSync(new URL(file, dir), "utf8")
    expect(code).not.toMatch(/^\s*["']use (client|server)["']/m)
    expect(code).not.toMatch(/from ["'](server-only|client-only|node:[^"']+|next(\/[^"']*)?|\.\.\/components[^"']*)["']/)
    expect(code).not.toMatch(/import ["'](server-only|client-only)["']/)
  })
})
