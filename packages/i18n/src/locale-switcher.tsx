"use client"

import { useRouter } from "next/navigation"
import { useLocale, useTranslations } from "next-intl"

import { LOCALE_COOKIE, locales } from "./locales"

/** Language picker: stores the choice in a cookie and re-renders the page in it. */
export function LocaleSwitcher({ className }: { className?: string }) {
  const t = useTranslations("common")
  const locale = useLocale()
  const router = useRouter()

  return (
    <select
      aria-label={t("language")}
      className={`h-8 rounded-lg border border-input bg-background px-2 text-sm ${className ?? ""}`}
      value={locale}
      onChange={(e) => {
        document.cookie = `${LOCALE_COOKIE}=${e.target.value}; path=/; max-age=31536000; samesite=lax`
        router.refresh()
      }}
    >
      {locales.map((l) => (
        <option key={l} value={l}>
          {t(`languages.${l}`)}
        </option>
      ))}
    </select>
  )
}
