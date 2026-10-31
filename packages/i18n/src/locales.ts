import type { AbstractIntlMessages } from "next-intl"

import en from "../messages/en.json"
import nl from "../messages/nl.json"
import sw from "../messages/sw.json"

export const locales = ["en", "nl", "sw"] as const
export type Locale = (typeof locales)[number]
export type Messages = AbstractIntlMessages

/** Cookie the language switcher writes; next-intl's own default name. */
export const LOCALE_COOKIE = "NEXT_LOCALE"

/** The cookie's locale when supported, else the app's default (field: sw). */
export function pickLocale(cookie: string | undefined, fallback: Locale): Locale {
  return locales.find((l) => l === cookie) ?? fallback
}

const shared: Record<Locale, Messages> = { en, nl, sw }

/** Shared texts (common, auth) plus the app's own namespaces. */
export async function loadMessages(
  locale: Locale,
  loadApp: (locale: Locale) => Promise<Messages>
): Promise<Messages> {
  return { ...shared[locale], ...(await loadApp(locale)) }
}
