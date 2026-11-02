import type { Metadata } from "next"
import { getTranslations } from "next-intl/server"

import { RootLayout } from "@/components/root-layout"

export async function generateMetadata(): Promise<Metadata> {
  const t = await getTranslations({ locale: "en", namespace: "landing" })
  return { title: t("title") }
}

export default function EnglishLayout({ children }: { children: React.ReactNode }) {
  return <RootLayout locale="en">{children}</RootLayout>
}
