"use client"

import { useLocale, useTranslations } from "next-intl"

import { useIncidentsScreen } from "@verzo/api-client/business-approvals"
import { IncidentsScreen } from "@verzo/ui/components/business-approvals/incidents-screen"
import type {
  IncidentDetailView,
  IncidentView,
} from "@verzo/ui/components/business-approvals/types"
import { PageHead } from "@/components/list"

/** Sync problems: Flow-run writes that did not arrive, were refused or changed (WF-03). */
export default function SyncProblemsPage() {
  const t = useTranslations("businessApprovals")
  const locale = useLocale()
  const s = useIncidentsScreen<IncidentView, IncidentDetailView>()
  return (
    <>
      <PageHead
        title={t("incidents.title")}
        description={t("incidents.intro")}
      />
      <IncidentsScreen
        s={s}
        t={t}
        locale={locale}
        runHref={(i) =>
          i.approval_request_id ? "/approvals/purchases" : undefined
        }
      />
    </>
  )
}
