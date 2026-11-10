"use client"

import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useStorefrontReview } from "@genesy/api-client/storefront-review"
import { ApiError } from "@genesy/ui/components/api-error"
import { Badge } from "@genesy/ui/components/badge"
import { Button } from "@genesy/ui/components/button"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { Notice } from "@genesy/ui/components/notice"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { StorefrontPage } from "@genesy/ui/components/storefront-page"
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
import {
  CardState,
  ListCard,
  ListPager,
  ListSkeleton,
  PageHead,
} from "@/components/list"
import { Done, LoadError, NoAccess, useDone } from "@/components/states"
import { useRights } from "@/lib/can"

/** Administration-only review of saved storefront designs. */
export default function StorefrontSettingsPage() {
  const workspaceId = useSession((s) => s.workspaceId)
  const token = useSession((s) => s.token)
  const rights = useRights()
  if (!token || !workspaceId || !rights.read) return null
  if (!rights.may("workspace_admin")) return <NoAccess />
  // Workspace/login changes discard preview and confirmation before another render.
  return (
    <StorefrontSettings
      key={`${workspaceId}:${token}`}
      workspaceId={workspaceId}
      token={token}
    />
  )
}

function StorefrontSettings({
  workspaceId,
  token,
}: {
  workspaceId: string
  token: string
}) {
  const t = useTranslations()
  const locale = useLocale()
  const done = useDone()
  const {
    state,
    revisions,
    preview,
    current,
    reviewed,
    supported,
    selected,
    selectRevision,
    setPage,
    ask,
    setAsk,
    confirm,
    conflict,
    error,
    busy,
    ready,
    canPublish,
    canRestore,
    revisionNumber,
  } = useStorefrontReview({
    workspaceId,
    token,
    canRender: canRenderStorefrontPage,
    onStart: done.clear,
    onDone: (kind) =>
      done.show(
        t(
          kind === "publish"
            ? "storefront.publishedDone"
            : "storefront.restoredDone"
        )
      ),
  })
  const columns = [
    { label: t("storefront.created") },
    { label: t("storefront.status") },
    { label: t("storefront.actions"), right: true },
  ]

  return (
    <>
      <PageHead
        eyebrow={t("shell.settings")}
        title={t("storefront.title")}
        description={t("storefront.intro")}
      />
      <Done message={done.message} />
      <ApiError
        // A 409 shows only the translated conflict notice below.
        error={error}
        fallback={t("common.error")}
      />
      {conflict && (
        <Notice variant="warning" title={t("storefront.conflict")} />
      )}
      {state.isError ? (
        <LoadError error={state.error} onRetry={() => void state.refetch()} />
      ) : null}
      <ListCard
        pager={
          revisions.data?.data.length ? (
            <ListPager meta={revisions.data.meta} onPage={setPage} />
          ) : null
        }
      >
        {state.isPending || revisions.isPending ? (
          <ListSkeleton columns={columns} />
        ) : revisions.isError ? (
          <CardState>
            <LoadError
              error={revisions.error}
              onRetry={() => void revisions.refetch()}
            />
          </CardState>
        ) : revisions.data.data.length === 0 ? (
          <CardState>
            <EmptyState title={t("storefront.empty")} />
          </CardState>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                {columns.map((column) => (
                  <TableHead
                    key={column.label}
                    className={column.right ? "text-right" : undefined}
                  >
                    {column.label}
                  </TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {revisions.data.data.map((revision, index) => (
                <TableRow key={revision.revision_id}>
                  <TableCell className="font-semibold">
                    <span>{formatDateTime(revision.created_at, locale)}</span>
                    <span className="mt-1 block font-mono text-xs text-muted-foreground">
                      {revision.revision_id.slice(0, 8)}
                    </span>
                  </TableCell>
                  <TableCell data-m="sub">
                    <span className="flex flex-wrap gap-1">
                      {revision.revision_id === current?.draft_revision_id ? (
                        <Badge variant="slate">{t("storefront.draft")}</Badge>
                      ) : null}
                      {revision.revision_id ===
                      current?.published_revision_id ? (
                        <Badge variant="teal">
                          {t("storefront.published")}
                        </Badge>
                      ) : revision.was_published ? (
                        <Badge variant="slate">
                          {t("storefront.previous")}
                        </Badge>
                      ) : null}
                    </span>
                  </TableCell>
                  <TableCell data-m="badge" className="text-right">
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={busy || state.isError}
                      aria-pressed={selected === revision.revision_id}
                      aria-label={t("storefront.previewRevision", {
                        number: revisionNumber(index),
                        date: formatDateTime(revision.created_at, locale),
                      })}
                      onClick={() => selectRevision(revision.revision_id)}
                    >
                      {t("storefront.preview")}
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </ListCard>
      {selected ? (
        <>
          {preview.isFetching ? (
            <Skeleton
              className="h-64 w-full"
              aria-label={t("common.loading")}
            />
          ) : preview.isError ? (
            <LoadError
              error={preview.error}
              onRetry={() => void preview.refetch()}
            />
          ) : reviewed && supported ? (
            <section
              aria-label={t("storefront.previewRegion")}
              className="flex min-w-0 flex-col gap-4"
            >
              <div className="flex flex-wrap items-center justify-between gap-3">
                <h2 className="text-lg font-semibold">
                  {t("storefront.previewRegion")}
                </h2>
                <div className="flex flex-wrap gap-2">
                  <Button
                    variant="outline"
                    disabled={busy || !canRestore}
                    onClick={() =>
                      setAsk({
                        kind: "restore",
                        revisionId: reviewed.revision_id,
                        version: reviewed.version,
                      })
                    }
                  >
                    {t("storefront.restore")}
                  </Button>
                  <Button
                    disabled={busy || !canPublish}
                    onClick={() =>
                      setAsk({
                        kind: "publish",
                        revisionId: reviewed.revision_id,
                        version: reviewed.version,
                      })
                    }
                  >
                    {t("storefront.publish")}
                  </Button>
                </div>
              </div>
              {current?.version !== reviewed.version ? (
                <Notice variant="warning" title={t("storefront.conflict")} />
              ) : null}
              <div className="min-w-0 overflow-hidden rounded-xl border bg-background p-4 md:p-6">
                <StorefrontPage
                  document={reviewed.document}
                  collections={reviewed.collections}
                  locale={locale}
                  preview
                  labels={{
                    inStock: t("storefront.inStock"),
                    low: t("storefront.lowStock"),
                    soldOut: t("storefront.soldOut"),
                    imageSoon: t("storefront.imageSoon"),
                    emptyProducts: t("storefront.emptyProducts"),
                    shopCta: t("storefront.shopCta"),
                  }}
                />
              </div>
            </section>
          ) : reviewed ? (
            <Notice variant="error" title={t("storefront.unsupported")} />
          ) : null}
        </>
      ) : null}
      {/* Separate dialogs with fixed wording: no text change while one closes. */}
      <ConfirmDialog
        open={ask?.kind === "publish"}
        onOpenChange={(open) => {
          if (!open) setAsk(null)
        }}
        title={t("storefront.publishTitle")}
        description={t("storefront.publishText")}
        confirm={t("storefront.publish")}
        cancel={t("actions.cancel")}
        destructive={false}
        disabled={busy || !ready}
        onConfirm={confirm}
      />
      <ConfirmDialog
        open={ask?.kind === "restore"}
        onOpenChange={(open) => {
          if (!open) setAsk(null)
        }}
        title={t("storefront.restoreTitle")}
        description={t("storefront.restoreText")}
        confirm={t("storefront.restore")}
        cancel={t("actions.cancel")}
        destructive={false}
        disabled={busy || !ready}
        onConfirm={confirm}
      />
    </>
  )
}
