import type { Metadata } from "next"
import { getTranslations } from "next-intl/server"

import type { Locale } from "@genesy/i18n/locales"
import { RootLayout } from "@/components/root-layout"

// English lives at `/`; only the other languages get a path segment, and nothing else matches.
export const dynamicParams = false
export function generateStaticParams() {
  return [{ locale: "nl" }, { locale: "sw" }]
}

export async function generateMetadata({
  params,
}: LayoutProps<"/[locale]">): Promise<Metadata> {
  const locale = (await params).locale as Locale
  const t = await getTranslations({ locale, namespace: "landing" })
  return { title: t("title") }
}

export default async function LocaleLayout({
  children,
  params,
}: LayoutProps<"/[locale]">) {
  return (
    <RootLayout locale={(await params).locale as Locale}>{children}</RootLayout>
  )
}
