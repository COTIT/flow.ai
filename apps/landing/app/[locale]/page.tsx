import type { Locale } from "@genesy/i18n/locales"
import { Landing } from "@/components/landing"

export default async function LocaleHome({ params }: PageProps<"/[locale]">) {
  return <Landing locale={(await params).locale as Locale} />
}
