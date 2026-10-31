import Link from "next/link"
import { getTranslations } from "next-intl/server"

import { LocaleSwitcher } from "@genesy/i18n/locale-switcher"
import { Logo } from "@genesy/ui/components/logo"
import { ThemeToggle } from "@genesy/ui/components/theme-toggle"

export async function SiteHeader() {
  const t = await getTranslations()
  return (
    <header>
      <div className="mx-auto flex w-full max-w-6xl flex-wrap items-center gap-x-4 gap-y-2 px-4 py-3 md:px-6">
        <Link
          href="/"
          aria-label={t("nav.home")}
          className="mr-auto rounded-sm"
        >
          <Logo aria-hidden className="h-7 md:h-9" />
        </Link>
        <nav className="hidden sm:block">
          <Link href="/" className="text-sm uppercase hover:underline">
            {t("nav.shop")}
          </Link>
        </nav>
        <LocaleSwitcher />
        <ThemeToggle label={t("common.theme")} />
      </div>
      <div className="gold-rule" />
    </header>
  )
}
