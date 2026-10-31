import { cookies } from "next/headers"
import { getRequestConfig } from "next-intl/server"

import { LOCALE_COOKIE, loadMessages, pickLocale, type Locale, type Messages } from "./locales"

/** An app's `i18n/request.ts`: locale from the cookie, else `defaultLocale`. */
export function createRequestConfig(
  defaultLocale: Locale,
  loadApp: (locale: Locale) => Promise<Messages>
) {
  return getRequestConfig(async () => {
    const locale = pickLocale((await cookies()).get(LOCALE_COOKIE)?.value, defaultLocale)
    return { locale, messages: await loadMessages(locale, loadApp) }
  })
}
