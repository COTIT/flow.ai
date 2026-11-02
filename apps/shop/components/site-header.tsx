import Link from "next/link"
import { getTranslations } from "next-intl/server"

import { LocaleSwitcher } from "@genesy/i18n/locale-switcher"
import { Logo } from "@genesy/ui/components/logo"
import { ThemeToggle } from "@genesy/ui/components/theme-toggle"

import { CartLink } from "./cart-link"

/** Mock `ShopCatalogue` .shop-head: one glass bar, logo, Shop, language, theme, cart. */
export async function SiteHeader() {
  const t = await getTranslations()
  return (
    <header className="relative flex items-center gap-2 rounded-[22px] glass-strong py-2 pr-2 pl-3.5 md:gap-5 md:py-3 md:pr-4 md:pl-[22px]">
      <Link
        href="/"
        aria-label={t("nav.home")}
        className="inline-flex min-h-11 items-center rounded-sm md:min-h-0"
      >
        <Logo aria-hidden className="h-auto w-[108px] min-w-0 md:w-[132px]" />
      </Link>
      <nav aria-label={t("nav.shop")} className="hidden gap-1 md:flex">
        <Link
          href="/"
          className="inline-flex h-[38px] items-center rounded-[12px] border border-border bg-(--glass-subtle) px-3.5 font-medium"
        >
          {t("nav.shop")}
        </Link>
      </nav>
      <span className="flex-1" />
      <LocaleSwitcher className="hidden h-10 border-transparent bg-transparent md:block" />
      <ThemeToggle
        label={t("common.theme")}
        className="size-10 rounded-[12px]"
      />
      <CartLink />
    </header>
  )
}
