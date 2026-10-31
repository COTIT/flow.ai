import { getTranslations } from "next-intl/server"

import { LocaleSwitcher } from "@genesy/i18n/locale-switcher"

export default async function Page() {
  const t = await getTranslations("app")
  return (
    <main className="flex min-h-svh flex-col gap-3 p-6">
      <h1 className="text-xl font-medium">{t("title")}</h1>
      <p className="text-sm text-muted-foreground">{t("tagline")}</p>
      <LocaleSwitcher className="w-fit" />
    </main>
  )
}
