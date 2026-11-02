import { getTranslations, setRequestLocale } from "next-intl/server"

import type { Locale } from "@genesy/i18n/locales"

/** The whole landing page for one language. Server-rendered; motion is added on top later. */
export async function Landing({ locale }: { locale: Locale }) {
  setRequestLocale(locale)
  const t = await getTranslations("landing")
  return (
    <main>
      <h1>{t("hero.title")}</h1>
    </main>
  )
}
