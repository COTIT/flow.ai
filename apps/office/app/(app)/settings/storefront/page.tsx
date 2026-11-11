"use client"

import * as React from "react"
import { useLocale, useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import { sessionStore } from "@genesy/api-client/session"
import {
  getStorefrontDesignGetQueryKey,
  getStorefrontDesignPreviewQueryKey,
  getStorefrontDesignRevisionsQueryKey,
  useStorefrontDesignGet,
  useStorefrontDesignPreview,
  useStorefrontDesignPublish,
  useStorefrontDesignRestore,
  useStorefrontDesignRevisions,
} from "@genesy/api-client/endpoints/storefront-design"
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
import { StorefrontConnection } from "@/components/storefront-connection"
import { useRights } from "@/lib/can"
import { useIdempotencyKey } from "@/lib/idempotency"

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
  const queryClient = useQueryClient()
  const [page, setPage] = React.useState(1)
  const [selected, setSelected] = React.useState<string | null>(null)
  const [ask, setAsk] = React.useState<{
    kind: "publish" | "restore"
    revisionId: string
    version: number
  } | null>(null)
  const [conflict, setConflict] = React.useState(false)
  const done = useDone()
  const state = useStorefrontDesignGet(workspaceId, { query: { retry: false } })
  const revisions = useStorefrontDesignRevisions(workspaceId, {
    page: String(page),
    page_size: "50",
  })
  const preview = useStorefrontDesignPreview(
    workspaceId,
    { revision_id: selected ?? "" },
    {
      query: { enabled: Boolean(selected), retry: false },
    }
  )
  const refresh = () =>
    Promise.all([
      queryClient.invalidateQueries({
        queryKey: getStorefrontDesignGetQueryKey(workspaceId),
      }),
      queryClient.invalidateQueries({
        queryKey: getStorefrontDesignRevisionsQueryKey(workspaceId),
      }),
      queryClient.invalidateQueries({
        queryKey: getStorefrontDesignPreviewQueryKey(workspaceId, {
          revision_id: selected ?? "",
        }).slice(0, 1),
      }),
    ])
  const clearReview = () => {
    setSelected(null)
    setAsk(null)
  }
  const changed = (error: unknown) => {
    if (
      (error as { response?: { status?: number } })?.response?.status === 409
    ) {
      clearReview()
      setConflict(true)
      void refresh()
    }
  }
  const publishKey = useIdempotencyKey()
  const restoreKey = useIdempotencyKey()
  const publish = useStorefrontDesignPublish({
    request: publishKey.request,
    mutation: {
      onMutate: done.clear,
      onSuccess: () => {
        clearReview()
        done.show(t("storefront.publishedDone"))
        return refresh()
      },
      onError: changed,
      onSettled: (_data, error) => publishKey.settle(error),
    },
  })
  const restore = useStorefrontDesignRestore({
    request: restoreKey.request,
    mutation: {
      onMutate: done.clear,
      onSuccess: () => {
        clearReview()
        done.show(t("storefront.restoredDone"))
        return refresh()
      },
      onError: changed,
      onSettled: (_data, error) => restoreKey.settle(error),
    },
  })
  const busy = publish.isPending || restore.isPending
  const current = state.data?.data
  const reviewed =
    selected && preview.data?.data.revision_id === selected
      ? preview.data.data
      : null
  const supported =
    reviewed && canRenderStorefrontPage(reviewed.document, reviewed.collections)
  const ready = Boolean(
    current &&
    reviewed &&
    supported &&
    current.version === reviewed.version &&
    !state.isFetching &&
    !preview.isFetching &&
    !preview.isError &&
    !state.isError
  )
  const canPublish =
    ready &&
    selected === current?.draft_revision_id &&
    selected !== current?.published_revision_id
  const canRestore = ready && selected !== current?.draft_revision_id
  const columns = [
    { label: t("storefront.created") },
    { label: t("storefront.status") },
    { label: t("storefront.actions"), right: true },
  ]
  const selectRevision = (revisionId: string) => {
    if (busy) return
    setAsk(null)
    setConflict(false)
    publish.reset()
    restore.reset()
    publishKey.renew()
    restoreKey.renew()
    done.clear()
    if (selected === revisionId) void preview.refetch()
    else setSelected(revisionId)
  }
  const confirm = () => {
    const session = sessionStore.getState()
    if (
      !ask ||
      busy ||
      !ready ||
      !reviewed ||
      ask.revisionId !== reviewed.revision_id ||
      ask.version !== reviewed.version ||
      session.workspaceId !== workspaceId ||
      session.token !== token ||
      (ask.kind === "publish" ? !canPublish : !canRestore)
    )
      return
    const variables = {
      workspace: workspaceId,
      data: { expected_version: ask.version, revision_id: ask.revisionId },
    }
    if (ask.kind === "publish") publish.mutate(variables)
    else restore.mutate(variables)
  }

  return (
    <>
      <PageHead
        eyebrow={t("shell.settings")}
        title={t("storefront.title")}
        description={t("storefront.intro")}
      />
      <StorefrontConnection workspaceId={workspaceId} token={token} />
      <Done message={done.message} />
      <ApiError
        // A 409 shows only the translated conflict notice below.
        error={conflict ? null : (publish.error ?? restore.error)}
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
            <ListPager
              meta={revisions.data.meta}
              onPage={(next) => {
                if (!busy) setPage(next)
              }}
            />
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
                        // Newest first and append-only, so numbers never shift.
                        number:
                          revisions.data.meta.total -
                          (revisions.data.meta.from ?? 1) +
                          1 -
                          index,
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
