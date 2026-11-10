import { createHash } from "node:crypto"
import { describe, expect, it } from "vitest"
import { locales } from "@genesy/i18n/locales"
import { loadOfficeMessages } from "./messages"

// Canonical JSON (sorted keys) so the pin checks content, not key order.
const canon = (v: unknown): unknown =>
  Array.isArray(v)
    ? v.map(canon)
    : v && typeof v === "object"
      ? Object.fromEntries(
          Object.keys(v)
            .sort()
            .map((k) => [k, canon((v as Record<string, unknown>)[k])])
        )
      : v
const hash = (v: unknown) =>
  createHash("sha256")
    .update(JSON.stringify(canon(v)))
    .digest("hex")

// Office's `voice` texts as they were in apps/office/messages at a432dec17.
const VOICE = {
  en: "f67b52c1b36b9b65360d6a613a2af6d2868e25e5e557743309f911721127bc67",
  nl: "ae3d1bccad3fd510366a4c5752b58a9f6f3e4d7025205a05f5fd57a780e3742a",
  sw: "76e478f6cf8efbe3d82560f8b91f5d4a1913780ad7f09cd6e36907ee56505691",
}

describe("Office messages", () => {
  it.each(locales)(
    "%s keeps the voice texts it had before the move",
    async (locale) => {
      const messages = await loadOfficeMessages(locale)
      expect(hash(messages.voice)).toBe(VOICE[locale])
    }
  )

  it.each(locales)(
    "%s gets voice from @genesy/i18n, not its own file",
    async (locale) => {
      const own = (await import(`../messages/${locale}.json`)).default
      expect(own).not.toHaveProperty("voice")
    }
  )
})
