"use client"

import * as React from "react"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
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
import { StorefrontFrame } from "./frame"

/** Owner/admin review of saved storefront designs; the API checks every call. */
export default function StorefrontReviewPage() {
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
}: {
  workspaceId: string
  token: string
}) {
  const t = useTranslations("flow.storefront")
  const tAll = useTranslations()
  const locale = useLocale()
  const [done, setDone] = React.useState<string | null>(null)
  const review = useStorefrontReview({
    workspaceId,
    token,
    canRender: canRenderStorefrontPage,
    onStart: () => setDone(null),
    onDone: (kind) =>
      setDone(t(kind === "publish" ? "publishedDone" : "restoredDone")),
  })
  const { state, revisions, preview, current, reviewed, selected, busy } =
    review
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
                          {revision.revision_id.slice(0, 8)}
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
      {/* Separate dialogs with fixed wording: no text change while one closes. */}
      <ConfirmDialog
        open={review.ask?.kind === "publish"}
        onOpenChange={(open) => {
          if (!open) review.setAsk(null)
        }}
        title={t("publishTitle")}
        description={t("publishText")}
        confirm={t("publish")}
        cancel={t("cancel")}
        destructive={false}
        disabled={busy || !review.ready}
        onConfirm={review.confirm}
      />
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
