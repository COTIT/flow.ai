import { readdirSync, readFileSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"

import { loadMessages, locales, pickLocale } from "./locales"

describe("pickLocale", () => {
  it("keeps a supported locale from the cookie", () => {
    expect(pickLocale("nl", "en")).toBe("nl")
  })

  it("falls back to the app's default for a missing or unknown cookie", () => {
    expect(pickLocale(undefined, "sw")).toBe("sw")
    expect(pickLocale("fr", "sw")).toBe("sw")
  })
})

describe("loadMessages", () => {
  it("merges shared texts with the app's own", async () => {
    const messages = await loadMessages("sw", async () => ({ app: { title: "Shamba" } }))
    expect(messages.app).toEqual({ title: "Shamba" })
    expect(messages.auth).toBeDefined()
  })
})

// Every messages folder in the repo: nl and sw must have exactly en's keys.
const root = join(import.meta.dirname, "../../..")
const folders = [
  join(root, "packages/i18n/messages"),
  ...readdirSync(join(root, "apps")).map((app) => join(root, "apps", app, "messages")),
].filter((dir) => {
  try {
    return readdirSync(dir).includes("en.json")
  } catch {
    return false
  }
})

function keys(value: unknown, prefix = ""): string[] {
  if (typeof value !== "object" || value === null) return [prefix]
  return Object.entries(value).flatMap(([k, v]) => keys(v, prefix ? `${prefix}.${k}` : k))
}

const read = (dir: string, locale: string) =>
  keys(JSON.parse(readFileSync(join(dir, `${locale}.json`), "utf8"))).sort()

describe.each(folders)("messages in %s", (dir) => {
  it.each(locales.filter((l) => l !== "en"))("%s has every en key and no extras", (locale) => {
    expect(read(dir, locale)).toEqual(read(dir, "en"))
  })
})
