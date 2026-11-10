import { createRequestConfig } from "@genesy/i18n/request"
import type { Locale, Messages } from "@genesy/i18n/locales"

/** Flow's own texts; shared common/auth come from @genesy/i18n. Extend here (e.g. voice) in one line. */
export const flowMessages = async (locale: Locale): Promise<Messages> =>
  (await import(`../messages/${locale}.json`)).default

export default createRequestConfig("en", flowMessages)
