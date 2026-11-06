import Link from "next/link"
import { getTranslations } from "next-intl/server"

import { LocaleSwitcher } from "@genesy/i18n/locale-switcher"
import { Logo } from "@genesy/ui/components/logo"
import { ThemeToggle } from "@genesy/ui/components/theme-toggle"

import { NAV } from "@/lib/nav"

import { CartLink } from "./cart-link"
import { MobileMenu } from "./mobile-menu"
import { NavLinks } from "./nav-links"

/** One glass bar: logo, the main links (a sheet on phones), language, theme, cart. */
export async function SiteHeader() {
  const t = await getTranslations()
  const labels = Object.fromEntries(
    NAV.map(({ key }) => [key, t(`nav.${key}`)])
  ) as React.ComponentProps<typeof NavLinks>["labels"]
  return (
    <header className="relative flex items-center gap-2 rounded-[22px] glass-strong py-2 pr-2 pl-3.5 md:gap-4 md:py-3 md:pr-4 md:pl-[22px]">
      <Link
        href="/"
        aria-label={t("nav.home")}
        className="inline-flex min-h-11 items-center rounded-sm md:min-h-0"
      >
        <Logo aria-hidden className="h-auto w-[108px] min-w-0 md:w-[132px]" />
      </Link>
      <nav aria-label={t("nav.main")} className="hidden gap-1 md:flex">
        <NavLinks labels={labels} />
      </nav>
      <span className="flex-1" />
      <LocaleSwitcher className="hidden h-10 border-transparent bg-transparent lg:block" />
      <ThemeToggle label={t("common.theme")} />
      <CartLink />
      <MobileMenu
        title={t("nav.main")}
        open={t("nav.openMenu")}
        labels={labels}
      >
        <LocaleSwitcher className="h-11 w-full" />
      </MobileMenu>
    </header>
  )
}
