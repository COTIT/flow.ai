"use client"

import * as React from "react"
import Link from "next/link"
import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  usePurchaseOrdersGet,
} from "@genesy/api-client/endpoints/purchase-orders"
import { Notice } from "@genesy/ui/components/notice"
import { DetailSkeleton } from "@/components/bento"
import { PageHead } from "@/components/list"
import { LoadError } from "@/components/states"

import { PoForm } from "../../po-form"

/** Change a draft (full replace); any other state says so and links back. */
export default function EditPurchasePage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const t = useTranslations()
  const { id } = React.use(params)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const order = usePurchaseOrdersGet(workspaceId, id, {
    query: { enabled: Boolean(workspaceId) },
  })
  if (order.isPending) return <DetailSkeleton />
  if (order.isError)
    return (
      <LoadError
        error={order.error}
        onRetry={() => void order.refetch()}
        backHref="/purchases"
      />
    )
  const o = order.data.data
  return (
    <>
      <PageHead
        back={{ href: `/purchases/${o.id}`, label: t("purchases.detailTitle", { number: o.number }) }}
        eyebrow={t("shell.buy")}
        title={t("purchases.editTitle", { number: o.number })}
      />
      {o.allowed_actions.includes("update") ? (
        <PoForm order={o} />
      ) : (
        <Notice
          variant="info"
          title={t("purchases.onlyDrafts")}
          action={
            <Link className="text-sm font-medium underline-offset-4 hover:underline" href={`/purchases/${o.id}`}>
              {t("purchases.backToOrder")}
            </Link>
          }
        />
      )}
    </>
  )
}
