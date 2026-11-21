"use client"

import * as React from "react"
import Link from "next/link"
import { useTranslations } from "next-intl"

import { useSession } from "@verzo/api-client/auth"
import { useSupplierBillsGet } from "@verzo/api-client/endpoints/supplier-bills"
import { Notice } from "@verzo/ui/components/notice"
import { DetailSkeleton } from "@/components/bento"
import { PageHead } from "@/components/list"
import { LoadError } from "@/components/states"

import { BillForm } from "../../bill-form"

/** Change a draft bill (full replace); any other state says so. */
export default function EditSupplierBillPage({ params }: { params: Promise<{ id: string }> }) {
  const t = useTranslations()
  const { id } = React.use(params)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const bill = useSupplierBillsGet(workspaceId, id, { query: { enabled: Boolean(workspaceId) } })
  if (bill.isPending) return <DetailSkeleton />
  if (bill.isError)
    return <LoadError error={bill.error} onRetry={() => void bill.refetch()} backHref="/supplier-bills" />
  const b = bill.data.data
  const editable = b.allowed_actions.includes("update")
  const back = `/supplier-bills/${b.id}`
  return (
    <>
      <PageHead
        back={{ href: back, label: t("payables.bills.detailTitle", { number: b.number }) }}
        eyebrow={t("shell.buy")}
        title={t("payables.bills.editTitle", { number: b.number })}
      />
      {editable ? (
        <BillForm bill={b} />
      ) : (
        <Notice
          variant="info"
          title={t("payables.bills.onlyDrafts")}
          action={
            <Link className="text-sm font-medium underline-offset-4 hover:underline" href={back}>
              {t("payables.bills.back")}
            </Link>
          }
        />
      )}
    </>
  )
}
