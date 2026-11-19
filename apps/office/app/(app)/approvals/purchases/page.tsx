"use client"

import { useLocale, useTranslations } from "next-intl"

import { usePurchaseApprovals } from "@genesy/api-client/business-approvals"
import { ApprovalsScreen } from "@genesy/ui/components/business-approvals/approvals-screen"
import type {
  ApprovalRequestView,
  ExecutionView,
} from "@genesy/ui/components/business-approvals/types"
import { PageHead } from "@/components/list"

/** Odoo purchase requests waiting for a decision before NetSuite gets the order (WF-01). */
export default function PurchaseApprovalsPage() {
  const t = useTranslations("businessApprovals")
  const locale = useLocale()
  const s = usePurchaseApprovals<ApprovalRequestView, ExecutionView>()
  return (
    <>
      <PageHead title={t("list.title")} description={t("list.intro")} />
      <ApprovalsScreen
        s={s}
        t={t}
        locale={locale}
        mappingHref={(b) =>
          b.code === "unmapped" && s.request
            ? `/settings/integrations/${s.request.pair_id}/mappings?entity=${b.entity ?? ""}`
            : undefined
        }
      />
    </>
  )
}
