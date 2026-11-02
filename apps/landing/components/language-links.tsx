import Link from "next/link"

import type { Locale } from "@genesy/i18n/locales"

const LANGUAGES: { locale: Locale; href: string; name: string }[] = [
  { locale: "en", href: "/", name: "English" },
  { locale: "nl", href: "/nl", name: "Nederlands" },
  { locale: "sw", href: "/sw", name: "Kiswahili" },
]

/** Plain links between /, /nl and /sw (each language has its own URL, for search engines). */
export function LanguageLinks({
  locale,
  short = false,
}: {
  locale: Locale
  short?: boolean
}) {
  return (
    <span className={short ? "langs langs-short" : "langs"}>
      {LANGUAGES.map((l) => (
        <Link
          key={l.locale}
          href={l.href}
          hrefLang={l.locale}
          lang={l.locale}
          aria-label={short ? l.name : undefined}
          aria-current={l.locale === locale ? "page" : undefined}
        >
          {short ? l.locale.toUpperCase() : l.name}
        </Link>
      ))}
    </span>
  )
}
