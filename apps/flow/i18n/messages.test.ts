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
