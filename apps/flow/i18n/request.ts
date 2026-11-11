import { createRequestConfig } from "@genesy/i18n/request"
import type { Locale, Messages } from "@genesy/i18n/locales"
import { voiceMessages } from "@genesy/i18n/voice"

/** Flow's own texts plus the shared voice assistant texts it hosts; common/auth come from @genesy/i18n. */
export const flowMessages = async (locale: Locale): Promise<Messages> => ({
  ...voiceMessages[locale],
  ...(await import(`../messages/${locale}.json`)).default,
})

export default createRequestConfig("en", flowMessages)
