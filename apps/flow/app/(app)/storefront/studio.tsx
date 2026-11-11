"use client"

import * as React from "react"
import { useLocale, useTranslations } from "next-intl"

import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getStorefrontDesignGetQueryKey,
  getStorefrontDesignRevisionsQueryKey,
} from "@genesy/api-client/endpoints/storefront-design"
import { useRights } from "@genesy/api-client/rights"
import { useStorefrontReview } from "@genesy/api-client/storefront-review"
import { ApiError } from "@genesy/ui/components/api-error"
import { Badge } from "@genesy/ui/components/badge"
import { Button } from "@genesy/ui/components/button"
import { Card, CardContent } from "@genesy/ui/components/card"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { Notice } from "@genesy/ui/components/notice"
import { Pager } from "@genesy/ui/components/pager"
import { Skeleton } from "@genesy/ui/components/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { formatDateTime } from "@genesy/ui/lib/format"
import { canRenderStorefrontPage } from "@genesy/ui/lib/storefront-design"
import { useVoice } from "@genesy/voice/components/provider"
import { useWorkspaceChoices } from "@/components/frame/header"
import { PreviewLinkCard } from "@/components/studio/preview-link-card"
import { PublishCard } from "@/components/studio/publish-card"
import {
  RedesignBrief,
  useSendWhenConnected,
} from "@/components/studio/redesign-brief"
import { revisionLabel, studioStore, useStudio } from "@/lib/studio"
import { StorefrontFrame } from "./frame"

/**
 * Storefront Studio: redesign through the assistant, an expiring full-page preview of the draft,
 * explicit publication, and the review of saved designs. Owner/admin only; the API checks every call.
 */
export function Studio({ shop }: { shop: string | null }) {
  const t = useTranslations("flow.storefront")
  const workspaceId = useSession((s) => s.workspaceId)
  const token = useSession((s) => s.token)
  const rights = useRights()
  if (!token || !workspaceId || !rights.read) return null
  if (!rights.may("workspace_admin"))
    return <Notice variant="info" title={t("noAccess")} />
  // Workspace/login changes discard preview and confirmation before another render.
  return (
    <Review
      key={`${workspaceId}:${token}`}
      workspaceId={workspaceId}
      token={token}
      shop={shop}
    />
  )
}

/** A failed load: the API's words (or a plain fallback) and a retry. */
function Failed({ error, onRetry }: { error: unknown; onRetry: () => void }) {
  const t = useTranslations()
  const message = (error as { response?: { data?: { message?: string } } })
    ?.response?.data?.message
  return (
    <Notice
      variant="error"
      title={message ?? t("common.error")}
      action={
        <Button variant="outline" size="sm" onClick={onRetry}>
          {t("flow.retry")}
        </Button>
      }
    />
  )
}

function Review({
  workspaceId,
  token,
  shop,
}: {
  workspaceId: string
  token: string
  shop: string | null
}) {
  const t = useTranslations("flow.storefront")
  const ts = useTranslations("studio")
  const tAll = useTranslations()
  const locale = useLocale()
  const voice = useVoice()
  const queryClient = useQueryClient()
  const send = useSendWhenConnected()
  const { choices } = useWorkspaceChoices()
  const [done, setDone] = React.useState<string | null>(null)
  const [brief, setBrief] = React.useState(false)
  const [hint, setHint] = React.useState(false)
  // Labels fixed when Confirm is pressed; shown once the API confirms.
  const asked = React.useRef<{ id: string; live: string; previous: string } | null>(null)
  const [receipt, setReceipt] = React.useState<typeof asked.current>(null)
  const review = useStorefrontReview({
    workspaceId,
    token,
    canRender: canRenderStorefrontPage,
    onStart: () => {
      setDone(null)
      setReceipt(null)
    },
    onDone: (kind) => {
      if (kind === "publish") setReceipt(asked.current)
      else setDone(t("restoredDone"))
    },
  })
  const { state, revisions, preview, current, reviewed, selected, busy } =
    review
  const label = (id: string | null | undefined) =>
    revisionLabel(revisions.data, id)
  const draftId = current?.draft_revision_id ?? null
  const liveId = current?.published_revision_id ?? null
  const unpublished = draftId && draftId !== liveId ? draftId : null
  const awaiting = useStudio((s) => s.awaiting)
  const preparing = Boolean(awaiting && awaiting.from === draftId)
  const publishing = review.ask?.kind === "publish" && Boolean(unpublished)
  const name = choices.find((w) => w.id === workspaceId)?.display_name ?? ""

  // A design review settled (approved, rejected or expired): its draft may exist now.
  const hadReview = React.useRef(false)
  React.useEffect(() => {
    if (hadReview.current && !voice.review) {
      void queryClient.invalidateQueries({
        queryKey: getStorefrontDesignGetQueryKey(workspaceId),
      })
      void queryClient.invalidateQueries({
        queryKey: getStorefrontDesignRevisionsQueryKey(workspaceId),
      })
    }
    hadReview.current = Boolean(voice.review)
  }, [voice.review, queryClient, workspaceId])

  const request = (text?: string) => {
    studioStore.setState({ awaiting: { from: draftId } })
    setBrief(false)
    setReceipt(null)
    if (text) send(text)
  }
  const reviewPublication = () => {
    if (!unpublished || !current || busy) return
    review.selectRevision(unpublished)
    review.setAsk({
      kind: "publish",
      revisionId: unpublished,
      version: current.version,
    })
  }
  const live =
    reviewed?.revision_id === current?.published_revision_id
      ? "isPublished"
      : reviewed?.revision_id === current?.draft_revision_id
        ? "isDraft"
        : "isOlder"

  return (
    <>
      <header className="flex flex-col gap-1">
        <h1 className="text-2xl font-semibold">{t("title")}</h1>
        <p className="text-sm text-muted-foreground">{t("intro")}</p>
      </header>
      <section aria-label={ts("region")} className="flex flex-col gap-3">
        {receipt ? (
          <section
            aria-label={ts("publishedTitle")}
            className="flex flex-col gap-1 rounded-2xl border bg-card p-4"
          >
            <p className="text-xs font-semibold tracking-widest text-muted-foreground uppercase">
              {ts("publishedTitle")}
            </p>
            <p className="text-sm">
              {ts("publishedText", {
                live: receipt.live,
                previous: receipt.previous,
              })}
            </p>
            {current?.published_revision_id === receipt.id ? (
              <p className="text-xs text-muted-foreground">
                {ts("publishedNote", { version: current.version })}
              </p>
            ) : null}
          </section>
        ) : null}
        {publishing && unpublished ? (
          <PublishCard
            live={label(liveId)}
            proposed={label(unpublished)}
            name={name}
            busy={busy}
            ready={review.ready && review.canPublish}
            onConfirm={() => {
              asked.current = {
                id: unpublished,
                live: label(unpublished),
                previous: label(liveId),
              }
              review.confirm()
            }}
            onCancel={() => review.setAsk(null)}
          />
        ) : preparing ? (
          <section
            aria-label={ts("preparingTitle")}
            className="flex flex-col gap-1 rounded-2xl border bg-card p-4"
          >
            <p className="text-xs font-semibold tracking-widest text-muted-foreground uppercase">
              {ts("preparingTitle")}
            </p>
            <p role="status" className="text-sm">
              {ts("preparingText", {
                live: label(liveId),
                draft: "—",
                status: ts(voice.review ? "statusReview" : "statusWorking"),
              })}
            </p>
          </section>
        ) : unpublished ? (
          <>
            <PreviewLinkCard
              workspaceId={workspaceId}
              draftId={unpublished}
              draftLabel={label(unpublished)}
              liveLabel={label(liveId)}
              shop={shop}
              blocked={Boolean(review.ask) || Boolean(voice.review)}
              onAnother={() => {
                request()
                setHint(true)
              }}
              onReview={reviewPublication}
            />
            {hint ? (
              <p role="status" className="text-sm text-muted-foreground">
                {ts("anotherHint")}
              </p>
            ) : null}
          </>
        ) : brief ? (
          <RedesignBrief onPrepare={request} />
        ) : state.isSuccess && revisions.isSuccess ? (
          <div className="flex flex-col items-start gap-2">
            <p className="text-sm text-muted-foreground">
              {ts("idle", { live: label(liveId) })}
            </p>
            <Button className="min-h-11" onClick={() => setBrief(true)}>
              {ts("start")}
            </Button>
          </div>
        ) : null}
      </section>
      <h2 className="text-lg font-semibold">{ts("history")}</h2>
      {done ? <Notice variant="success" title={done} /> : null}
      <ApiError error={review.error} fallback={tAll("common.error")} />
      {review.conflict ? (
        <Notice variant="warning" title={t("conflict")} />
      ) : null}
      {state.isError ? (
        <Failed error={state.error} onRetry={() => void state.refetch()} />
      ) : null}
      <Card>
        <CardContent className="flex flex-col gap-3">
          {state.isPending || revisions.isPending ? (
            <Skeleton
              className="h-32 w-full"
              aria-label={tAll("common.loading")}
            />
          ) : revisions.isError ? (
            <Failed
              error={revisions.error}
              onRetry={() => void revisions.refetch()}
            />
          ) : revisions.data.data.length === 0 ? (
            <EmptyState title={t("empty")} />
          ) : (
            <>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>{t("created")}</TableHead>
                    <TableHead>{t("status")}</TableHead>
                    <TableHead className="text-right">{t("actions")}</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {revisions.data.data.map((revision, index) => (
                    <TableRow key={revision.revision_id}>
                      <TableCell className="font-semibold">
                        <span>
                          {formatDateTime(revision.created_at, locale)}
                        </span>
                        <span className="mt-1 block font-mono text-xs text-muted-foreground">
                          {revision.revision_id.slice(-8)}
                        </span>
                      </TableCell>
                      <TableCell>
                        <span className="flex flex-wrap gap-1">
                          {revision.revision_id ===
                          current?.draft_revision_id ? (
                            <Badge variant="slate">{t("draft")}</Badge>
                          ) : null}
                          {revision.revision_id ===
                          current?.published_revision_id ? (
                            <Badge variant="teal">{t("published")}</Badge>
                          ) : revision.was_published ? (
                            <Badge variant="slate">{t("previous")}</Badge>
                          ) : null}
                        </span>
                      </TableCell>
                      <TableCell className="text-right">
                        <Button
                          variant="outline"
                          size="sm"
                          disabled={busy || state.isError}
                          aria-pressed={selected === revision.revision_id}
                          aria-label={t("previewRevision", {
                            number: review.revisionNumber(index),
                            date: formatDateTime(revision.created_at, locale),
                          })}
                          onClick={() =>
                            review.selectRevision(revision.revision_id)
                          }
                        >
                          {t("preview")}
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
              {revisions.data.meta.last_page > 1 ? (
                <Pager
                  page={revisions.data.meta.current_page}
                  lastPage={revisions.data.meta.last_page}
                  summary={t("page", {
                    page: revisions.data.meta.current_page,
                    last: revisions.data.meta.last_page,
                  })}
                  previous={t("previousPage")}
                  next={t("nextPage")}
                  onPage={review.setPage}
                />
              ) : null}
            </>
          )}
        </CardContent>
      </Card>
      {selected ? (
        preview.isFetching ? (
          <Skeleton
            className="h-64 w-full"
            aria-label={tAll("common.loading")}
          />
        ) : preview.isError ? (
          <Failed
            error={preview.error}
            onRetry={() => void preview.refetch()}
          />
        ) : reviewed && review.supported ? (
          <section
            aria-label={t("previewRegion")}
            className="flex min-w-0 flex-col gap-4"
          >
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="flex min-w-0 flex-col gap-1">
                <h2 className="text-lg font-semibold">{t("previewRegion")}</h2>
                <p className="text-sm font-medium">{t(live)}</p>
              </div>
              <div className="flex flex-wrap gap-2">
                <Button
                  variant="outline"
                  disabled={busy || !review.canRestore}
                  onClick={() =>
                    review.setAsk({
                      kind: "restore",
                      revisionId: reviewed.revision_id,
                      version: reviewed.version,
                    })
                  }
                >
                  {t("restore")}
                </Button>
                <Button
                  disabled={busy || !review.canPublish}
                  onClick={() =>
                    review.setAsk({
                      kind: "publish",
                      revisionId: reviewed.revision_id,
                      version: reviewed.version,
                    })
                  }
                >
                  {t("publish")}
                </Button>
              </div>
            </div>
            {current?.version !== reviewed.version ? (
              <Notice variant="warning" title={t("conflict")} />
            ) : null}
            <StorefrontFrame
              // A new workspace, login or revision loads a fresh frame document.
              key={`${workspaceId}:${token}:${reviewed.revision_id}`}
              revisionId={reviewed.revision_id}
            />
          </section>
        ) : reviewed ? (
          <Notice variant="error" title={t("unsupported")} />
        ) : null
      ) : null}
      {/* Publish has its own card above; restore keeps its own dialog with fixed wording. */}
      <ConfirmDialog
        open={review.ask?.kind === "restore"}
        onOpenChange={(open) => {
          if (!open) review.setAsk(null)
        }}
        title={t("restoreTitle")}
        description={t("restoreText")}
        confirm={t("restore")}
        cancel={t("cancel")}
        destructive={false}
        disabled={busy || !review.ready}
        onConfirm={review.confirm}
      />
    </>
  )
}
