"use client"

import { useTranslations } from "next-intl"

export default function SalePage() {
  const t = useTranslations()
  return <h1 className="text-xl font-medium">{t("sale.title")}</h1>
}
