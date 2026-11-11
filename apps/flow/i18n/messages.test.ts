import { describe, expect, it } from "vitest"
import { loadMessages, locales } from "@genesy/i18n/locales"
import { voiceMessages } from "@genesy/i18n/voice"
import { flowMessages } from "./request"

describe("Flow messages", () => {
  it.each(locales)(
    "%s hosts the shared voice texts and keeps Flow's own",
    async (locale) => {
      const messages = await loadMessages(locale, flowMessages)
      expect(messages.voice).toEqual(voiceMessages[locale].voice)
      expect(messages.flow).toBeDefined()
    }
  )
})

const keys = (value: unknown, prefix = ""): string[] =>
  value && typeof value === "object"
    ? Object.entries(value).flatMap(([key, item]) =>
        keys(item, `${prefix}${key}.`)
      )
    : [prefix]

describe("Flow namespace files", () => {
  it.each(locales)("%s merges shell.json with the same keys as English", async (locale) => {
    const english = await loadMessages("en", flowMessages)
    const messages = await loadMessages(locale, flowMessages)
    expect(english.shell).toBeDefined()
    expect(keys(messages.shell).sort()).toEqual(keys(english.shell).sort())
  })
})
