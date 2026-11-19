"use client"

import { useTranslations } from "next-intl"

import { PageHead } from "@/components/list"
import { NoAccess } from "@/components/states"
import { useCan, useRights } from "@/lib/can"

import { BillForm } from "../bill-form"

/** A new draft supplier bill (Form card template). */
export default function NewSupplierBillPage() {
  const t = useTranslations()
  const rights = useRights()
  const may = useCan("supplierbill.create")
  if (rights.read && !may) return <NoAccess />
  return (
    <>
      <PageHead
        back={{ href: "/supplier-bills", label: t("payables.bills.title") }}
        eyebrow={t("shell.buy")}
        title={t("payables.bills.newTitle")}
      />
      <BillForm />
    </>
  )
}
