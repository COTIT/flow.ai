"use client"

import Link from "next/link"
import { usePathname } from "next/navigation"
import { useTranslations } from "next-intl"

const tabs = ["links", "transfers"] as const

/** Supply from partner workspaces: one heading and a tab row. */
export default function SupplyLayout({
  children,
}: {
  children: React.ReactNode
}) {
  const t = useTranslations("supply")
  const pathname = usePathname()
  return (
    <section className="flex flex-col gap-3">
      <h1 className="heading-2 text-lg">{t("title")}</h1>
      <nav aria-label={t("title")} className="-mx-1 flex gap-1 overflow-x-auto">
        {tabs.map((tab) => (
          <Link
            key={tab}
            href={`/supply/${tab}`}
            aria-current={
              pathname.startsWith(`/supply/${tab}`) ? "page" : undefined
            }
            className="shrink-0 rounded-lg px-3 py-1.5 text-sm text-muted-foreground hover:bg-muted aria-[current=page]:bg-secondary aria-[current=page]:font-medium aria-[current=page]:text-foreground"
          >
            {t(`tabs.${tab}`)}
          </Link>
        ))}
      </nav>
      {children}
    </section>
  )
}
