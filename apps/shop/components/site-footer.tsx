import { getTranslations } from "next-intl/server"

import { LogoIcon } from "@genesy/ui/components/logo"

export async function SiteFooter() {
  const t = await getTranslations()
  return (
    <footer className="bg-primary text-primary-foreground dark:bg-card dark:text-card-foreground">
      <div className="gold-rule" />
      <div className="mx-auto flex w-full max-w-6xl items-center gap-4 px-4 py-8 md:px-6">
        <LogoIcon aria-hidden />
        <p className="text-sm">{t("footer.note")}</p>
      </div>
    </footer>
  )
}
