"use client"

import * as React from "react"
import { useQuery } from "@tanstack/react-query"
import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { storefrontDesignEditPreview } from "@genesy/api-client/endpoints/storefront-design"
import type { StorefrontDesignEdit } from "@genesy/api-client/model"
import { Badge } from "@genesy/ui/components/badge"
import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { useVoice } from "@genesy/voice/components/provider"
import type { VoiceReview } from "@genesy/voice/lib/client-contract"
import { StorefrontFrame } from "@/app/(app)/storefront/frame"
import { holdsApproval } from "@/lib/storefront-proposal"

const kinds = {
  storefront_design_edit: "edit",
  storefront_design_restore: "restore",
  storefront_design_publish: "publish",
} as const
type Kind = (typeof kinds)[keyof typeof kinds]

export const designKind = (review: VoiceReview | null): Kind | null =>
  (review?.operationId && kinds[review.operationId as keyof typeof kinds]) ||
  null

const status = (error: unknown) =>
  (error as { response?: { status?: number } } | null)?.response?.status

/**
 * The API's dry run of an edit review: the exact request body the approval will send
 * (`details.changes`) applied to the current draft; nothing is stored. One query per review, shared
 * by the hold below and the preview, so it runs once.
 */
function useDryRun(review: VoiceReview | null) {
  const workspaceId = useSession((s) => s.workspaceId)
  const edit = designKind(review) === "edit"
  return useQuery({
    queryKey: ["storefront-proposal", workspaceId, review?.ticket],
    queryFn: ({ signal }) =>
      storefrontDesignEditPreview(
        workspaceId!,
        review!.details.changes as StorefrontDesignEdit,
        undefined,
        signal
      ),
    enabled: edit && Boolean(workspaceId),
    retry: false,
    staleTime: Infinity,
  })
}

/**
 * Holds approval of an edit review while its dry run runs and when it says the draft moved (409),
 * so neither the button nor the spoken phrase can send it; any other answer releases it (rule:
 * `holdsApproval`). The hold names this ticket, so the next review is free. Mounted with the
 * conversation, so it holds with the sheet closed too; set before paint, so no click lands first.
 */
export function useProposalHold(review: VoiceReview | null) {
  const { holdApproval } = useVoice()
  const dryRun = useDryRun(review)
  const held = holdsApproval(
    designKind(review) === "edit",
    dryRun.status,
    status(dryRun.error)
  )
  const ticket = review?.ticket
  React.useLayoutEffect(() => {
    holdApproval(held && ticket ? ticket : null)
  }, [held, ticket, holdApproval])
}

/** What the shop's home page looks like if this review is approved; the diff above stays the review. */
export function StorefrontProposal({
  review,
  kind,
}: {
  review: VoiceReview
  kind: Kind
}) {
  const t = useTranslations("flow.storefront")
  const changes = review.details.changes as
    { revision_id?: unknown } | undefined
  return (
    <section
      aria-labelledby="flow-proposal-title"
      className="flex min-w-0 flex-col gap-3"
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h4 id="flow-proposal-title" className="text-sm font-semibold">
          {t(`${kind}PreviewTitle`)}
        </h4>
        <Badge variant={kind === "publish" ? "gold" : "slate"}>
          {t(`${kind}PreviewBadge`)}
        </Badge>
      </div>
      <p className="text-sm text-muted-foreground">{t(`${kind}PreviewText`)}</p>
      {kind === "edit" ? (
        <EditPreview review={review} />
      ) : typeof changes?.revision_id === "string" ? (
        <StorefrontFrame revisionId={changes.revision_id} />
      ) : (
        <Notice variant="error" title={t("unavailable")} />
      )}
    </section>
  )
}

function EditPreview({ review }: { review: VoiceReview }) {
  const t = useTranslations("flow.storefront")
  const dryRun = useDryRun(review)
  if (dryRun.isPending)
    return <Skeleton className="h-64 w-full" aria-label={t("previewLoading")} />
  if (dryRun.isError)
    return status(dryRun.error) === 409 ? (
      <Notice variant="warning" title={t("proposalConflict")}>
        {t("proposalConflictText")}
      </Notice>
    ) : (
      <Notice
        variant="error"
        title={t("unavailable")}
        action={
          <Button
            variant="outline"
            size="sm"
            className="min-h-11"
            disabled={dryRun.isFetching}
            onClick={() => void dryRun.refetch()}
          >
            {t("retry")}
          </Button>
        }
      >
        {t("proposalUnavailableText")}
      </Notice>
    )
  // The query's own object: stable across renders, so the frame's listener is set up once.
  return <StorefrontFrame proposal={dryRun.data.data} />
}
