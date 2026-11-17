"use client"

import { useLocale, useTranslations } from "next-intl"

import { usePurchaseApprovals } from "@genesy/api-client/business-approvals"
import { ApprovalsScreen } from "@genesy/ui/components/business-approvals/approvals-screen"
import type {
  ApprovalRequestView,
  ExecutionView,
} from "@genesy/ui/components/business-approvals/types"

/**
 * Odoo purchase requests to decide, in Flow (WF-01). Not the agent-task approval scene
 * (components/task/approval-scene.tsx), which stays separate. Mapping fixes live in Office.
 */
export function PurchaseApprovals({ office }: { office: string | null }) {
  const t = useTranslations("businessApprovals")
  const locale = useLocale()
  const s = usePurchaseApprovals<ApprovalRequestView, ExecutionView>()
  return (
    <section className="mx-auto flex w-full max-w-5xl flex-col gap-5 p-1 pb-8">
      <div className="flex flex-col gap-1">
        <h1 className="text-2xl font-semibold tracking-tight">
          {t("list.title")}
        </h1>
        <p className="text-sm text-muted-foreground">{t("list.intro")}</p>
      </div>
      <ApprovalsScreen
        s={s}
        t={t}
        locale={locale}
        mappingHref={(b) =>
          office && b.code === "unmapped" && s.request
            ? `${office}/settings/integrations/${s.request.pair_id}/mappings?entity=${b.entity ?? ""}`
            : undefined
        }
      />
    </section>
  )
}
