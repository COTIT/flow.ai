import { getRequestConfig } from "next-intl/server"

import { loadMessages, locales, type Locale } from "@genesy/i18n/locales"

// The landing page puts the language in the path (/, /nl, /sw) for search engines, not in a cookie.
export default getRequestConfig(async ({ requestLocale }) => {
  const asked = await requestLocale
  const locale: Locale = locales.find((l) => l === asked) ?? "en"
  return {
    locale,
    messages: await loadMessages(
      locale,
      async (l) => (await import(`../messages/${l}.json`)).default
    ),
  }
})
