import { createRequestConfig } from "@verzo/i18n/request"
import type { Locale, Messages } from "@verzo/i18n/locales"
import { voiceMessages } from "@verzo/i18n/voice"

/** One file per area (`messages/<locale>/<name>.json`), so each change owns its own texts. */
const NAMESPACES = [
  "shell",
  "scenes",
  "discovery",
  "task",
  "studio",
  "history",
  "product-bulk",
] as const // F2 scenes, F3 discovery, F4 task, F5 studio

/** Flow's own texts plus the shared voice assistant texts it hosts; common/auth come from @verzo/i18n. */
export const flowMessages = async (locale: Locale): Promise<Messages> => {
  const parts = await Promise.all(
    NAMESPACES.map(
      async (name) =>
        (await import(`../messages/${locale}/${name}.json`)).default as Messages
    )
  )
  return Object.assign(
    { ...voiceMessages[locale] },
    (await import(`../messages/${locale}.json`)).default,
    ...parts
  )
}

export default createRequestConfig("en", flowMessages)
