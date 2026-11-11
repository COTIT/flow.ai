"use client"

import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  useStorefrontDesignGet,
  useStorefrontDesignPreviewLinks,
  useStorefrontDesignRevisions,
} from "@genesy/api-client/endpoints/storefront-design"
import { useRights } from "@genesy/api-client/rights"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { formatDateTime } from "@genesy/ui/lib/format"
import { revisionLabel, studioStore, useStudio } from "@/lib/studio"

/**
 * Home's storefront prompt, from persisted revision state only (no voice): an unpublished draft
 * waits for review, or this tab's request is still being prepared. Owners and admins only.
 */
export function StudioResume() {
  const workspaceId = useSession((s) => s.workspaceId)
  const rights = useRights()
  const dismissed = useStudio((s) => s.dismissed)
  if (!workspaceId || !rights.read || !rights.may("workspace_admin") || dismissed)
    return null
  return <Prompt key={workspaceId} workspaceId={workspaceId} />
}

function Prompt({ workspaceId }: { workspaceId: string }) {
  const t = useTranslations("studio")
  const locale = useLocale()
  const state = useStorefrontDesignGet(workspaceId, { query: { retry: false } })
  const revisions = useStorefrontDesignRevisions(workspaceId, {
    page: "1",
    page_size: "50",
  })
  const awaiting = useStudio((s) => s.awaiting)
  const current = state.data?.data
  const draftId = current?.draft_revision_id ?? null
  const liveId = current?.published_revision_id ?? null
  const unpublished = draftId && draftId !== liveId ? draftId : null
  const links = useStorefrontDesignPreviewLinks(
    workspaceId,
    { revision_id: unpublished ?? "" },
    { query: { enabled: Boolean(unpublished) } }
  )
  if (!current || !revisions.data) return null
  const label = (id: string | null) => revisionLabel(revisions.data, id)
  const box = "flex w-full max-w-md flex-col gap-2 rounded-2xl border bg-card p-4 text-left"
  const kicker = "text-xs font-semibold tracking-widest text-muted-foreground uppercase"

  if (awaiting && awaiting.from === draftId)
    return (
      <section aria-label={t("preparingTitle")} className={box}>
        <p className={kicker}>{t("preparingTitle")}</p>
        <p className="text-sm">
          {t("preparingText", {
            live: label(liveId),
            draft: "—",
            status: t("statusWorking"),
          })}
        </p>
        <Link href="/storefront" className={buttonVariants({ variant: "outline", className: "min-h-11 self-start" })}>
          {t("openStudio")}
        </Link>
      </section>
    )
  if (!unpublished) return null
  const active = links.data?.data[0]?.status === "active" ? links.data.data[0] : null
  return (
    <section aria-label={t("continueTitle")} className={box}>
      <p className={kicker}>{t("continueTitle")}</p>
      <p className="text-sm">{t("continueText", { draft: label(unpublished) })}</p>
      <p className="text-sm text-muted-foreground">
        {active
          ? t("continueExpires", { date: formatDateTime(active.expires_at, locale) })
          : t("continueNoLink")}
      </p>
      <div className="flex flex-wrap gap-2">
        <Link href="/storefront" className={buttonVariants({ className: "min-h-11" })}>
          {t("continue")}
        </Link>
        <Button
          variant="outline"
          className="min-h-11"
          onClick={() => studioStore.setState({ dismissed: true })}
        >
          {t("dismiss")}
        </Button>
      </div>
    </section>
  )
}
