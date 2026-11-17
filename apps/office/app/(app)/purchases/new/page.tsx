"use client"

import { useTranslations } from "next-intl"

import { PageHead } from "@/components/list"
import { NoAccess } from "@/components/states"
import { useCan, useRights } from "@/lib/can"

import { PoForm } from "../po-form"

/** A new draft purchase order (Form card template). */
export default function NewPurchasePage() {
  const t = useTranslations()
  const rights = useRights()
  const may = useCan("purchaseorder.create")
  if (rights.read && !may) return <NoAccess />
  return (
    <>
      <PageHead
        back={{ href: "/purchases", label: t("purchases.title") }}
        eyebrow={t("shell.buy")}
        title={t("purchases.newTitle")}
      />
      <PoForm />
    </>
  )
}
