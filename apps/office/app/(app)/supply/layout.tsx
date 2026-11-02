"use client"

import { usePathname } from "next/navigation"
import { useTranslations } from "next-intl"

import { PageHead, PillLinks } from "@/components/list"

const tabs = ["links", "transfers"] as const

/** Supply from partner workspaces: one heading and a tab row. */
export default function SupplyLayout({
  children,
}: {
  children: React.ReactNode
}) {
  const t = useTranslations()
  const pathname = usePathname()
  return (
    <>
      <PageHead
        eyebrow={t("shell.trace")}
        title={t("supply.title")}
        tabs={
          <PillLinks
            label={t("supply.title")}
            links={tabs.map((tab) => ({
              href: `/supply/${tab}`,
              label: t(`supply.tabs.${tab}`),
              active: pathname.startsWith(`/supply/${tab}`),
            }))}
          />
        }
      />
      {children}
    </>
  )
}
