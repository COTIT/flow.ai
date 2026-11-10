import { type Locale, type Messages, loadMessages } from "@genesy/i18n/locales"
import { voiceMessages } from "@genesy/i18n/voice"

/** Office's own texts plus the shared voice assistant texts it hosts. */
export const loadOfficeApp = async (locale: Locale): Promise<Messages> => ({
  ...voiceMessages[locale],
  ...(await import(`../messages/${locale}.json`)).default,
})

export const loadOfficeMessages = (locale: Locale) =>
  loadMessages(locale, loadOfficeApp)
