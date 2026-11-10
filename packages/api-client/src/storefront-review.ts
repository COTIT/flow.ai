"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"

import {
  getStorefrontDesignGetQueryKey,
  getStorefrontDesignPreviewQueryKey,
  getStorefrontDesignRevisionsQueryKey,
  useStorefrontDesignGet,
  useStorefrontDesignPreview,
  useStorefrontDesignPublish,
  useStorefrontDesignRestore,
  useStorefrontDesignRevisions,
} from "./generated/storefront-design/storefront-design"
import { useIdempotencyKey } from "./idempotency"
import { sessionStore } from "./session"

/**
 * The administration review of saved storefront designs, shared by Office and Flow: state,
 * revisions, one selected preview, and Publish/Restore behind a confirmation that re-checks
 * the reviewed version, login and workspace. A 409 clears the review and sets `conflict`.
 * Mount it under a key of workspace and token, so a switch discards review and confirmation.
 */
export function useStorefrontReview({
  workspaceId,
  token,
  canRender,
  onStart,
  onDone,
}: {
  workspaceId: string
  token: string
  /** Whether the shared renderer supports the reviewed document (`canRenderStorefrontPage`). */
  canRender: (document: unknown, collections: unknown) => boolean
  /** A write starts, or another revision is picked: clear the last success message. */
  onStart: () => void
  /** A write succeeded. */
  onDone: (kind: "publish" | "restore") => void
}) {
  const queryClient = useQueryClient()
  const [page, setPage] = React.useState(1)
  const [selected, setSelected] = React.useState<string | null>(null)
  const [ask, setAsk] = React.useState<{
    kind: "publish" | "restore"
    revisionId: string
    version: number
  } | null>(null)
  const [conflict, setConflict] = React.useState(false)
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
      onMutate: onStart,
      onSuccess: () => {
        clearReview()
        onDone("publish")
        return refresh()
      },
      onError: changed,
      onSettled: (_data, error) => publishKey.settle(error),
    },
  })
  const restore = useStorefrontDesignRestore({
    request: restoreKey.request,
    mutation: {
      onMutate: onStart,
      onSuccess: () => {
        clearReview()
        onDone("restore")
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
  const supported = Boolean(
    reviewed && canRender(reviewed.document, reviewed.collections)
  )
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
  const selectRevision = (revisionId: string) => {
    if (busy) return
    setAsk(null)
    setConflict(false)
    publish.reset()
    restore.reset()
    publishKey.renew()
    restoreKey.renew()
    onStart()
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
  /** Newest first and append-only, so numbers never shift. */
  const revisionNumber = (index: number) =>
    revisions.data
      ? revisions.data.meta.total - (revisions.data.meta.from ?? 1) + 1 - index
      : 0

  return {
    state,
    revisions,
    preview,
    current,
    reviewed,
    supported,
    selected,
    selectRevision,
    page,
    setPage: (next: number) => {
      if (!busy) setPage(next)
    },
    ask,
    setAsk,
    confirm,
    conflict,
    /** The write error to show; a 409 shows only the conflict notice. */
    error: conflict ? null : (publish.error ?? restore.error),
    busy,
    ready,
    canPublish,
    canRestore,
    revisionNumber,
  }
}
