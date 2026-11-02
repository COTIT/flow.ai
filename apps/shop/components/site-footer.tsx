import { getTranslations } from "next-intl/server"

import { LocaleSwitcher } from "@genesy/i18n/locale-switcher"
import { LogoIcon } from "@genesy/ui/components/logo"

/** Mock `ShopCatalogue` .foot: navy card (a quiet glass edge in dark mode). */
export async function SiteFooter() {
  const t = await getTranslations("footer")
  return (
    <footer className="flex flex-wrap items-center justify-between gap-6 rounded-[24px] bg-primary p-7 text-primary-foreground dark:border dark:border-(--hair) dark:bg-white/4 dark:text-foreground">
      <div className="flex items-center gap-4">
        <LogoIcon aria-hidden className="h-auto w-[42px]" />
        <div>
          <p className="font-semibold">{t("title")}</p>
          <p className="text-[13px] opacity-80">{t("shipping")}</p>
        </div>
      </div>
      <LocaleSwitcher className="h-11 border-white/30 bg-transparent text-[13px] text-inherit md:h-10 [&>option]:text-[#0a183a]" />
    </footer>
  )
}
