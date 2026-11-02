"use client"

import { usePathname } from "next/navigation"
import { useTranslations } from "next-intl"

import { PageHead, PillLinks } from "@/components/list"

const tabs = ["registers", "users", "vaults", "shifts", "sales"] as const

/** The till admin screens share one heading and a tab row. */
export default function TillLayout({
  children,
}: {
  children: React.ReactNode
}) {
  const t = useTranslations()
  const pathname = usePathname()
  return (
    <>
      <PageHead
        eyebrow={t("shell.store")}
        title={t("till.title")}
        tabs={
          <PillLinks
            label={t("till.title")}
            links={tabs.map((tab) => ({
              href: `/till/${tab}`,
              label: t(`till.tabs.${tab}`),
              active: pathname.startsWith(`/till/${tab}`),
            }))}
          />
        }
      />
      {children}
    </>
  )
}
